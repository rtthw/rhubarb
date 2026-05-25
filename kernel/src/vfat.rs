//! # Virtual File Allocation Table (VFAT)

use {
    crate::loader,
    alloc::{
        collections::vec_deque::VecDeque,
        string::{String, ToString as _},
        vec::Vec,
    },
    boot_info::BootInfo,
    core::fmt,
    fs::{DirectoryEntry, FileSystem},
    hashbrown::HashMap,
    io::BlockReader,
    log::{debug, error},
};

const SECTOR_SIZE: usize = 512;
const ENTRIES_PER_SECTOR: usize = SECTOR_SIZE / size_of::<VfatDirectoryEntry>();
const MAX_CLUSTER_INDEX: u32 = 0xFFF8;
const BAD_CLUSTER_INDEX: u32 = 0xFFF7;



pub fn init(boot_info: &BootInfo, drive: &mut ata::Drive, lba_start: u32, lba_sector_count: u32) {
    let mut buf = [0; SECTOR_SIZE];
    drive
        .read_blocks(lba_start as usize, &mut buf)
        .map_err(|_| "failed to read VFAT boot sector")
        .unwrap();

    let boot_sector: VfatBootSector = unsafe { core::mem::transmute(buf) };
    // debug!("{boot_sector:#?}");

    if boot_sector.sector_count() != lba_sector_count as usize {
        error!("Boot sector does not have the same number of sectors as its partition");
        return;
    }

    let fs = VfatFileSystem::new(drive.clone(), boot_sector, lba_start).unwrap();

    loader::init(boot_info, fs);
}

// https://wiki.osdev.org/FAT#FAT_32_and_exFAT
fn read_file_bytes(
    drive: &mut ata::Drive,
    lba_start: u32,
    boot_sector: &VfatBootSector,
    first_cluster: usize,
    file_size: usize,
) -> Result<Vec<u8>, &'static str> {
    let mut bytes = vec![0; file_size];
    let mut byte_offset = 0;
    let cluster_size = boot_sector.sectors_per_cluster as usize * SECTOR_SIZE;

    let mut current_cluster = first_cluster;
    while current_cluster < MAX_CLUSTER_INDEX as usize {
        let cluster_offset = boot_sector.data_sector_offset()
            + ((current_cluster - 2) * boot_sector.sectors_per_cluster());

        // log::trace!("READ_CLUSTER @ {current_cluster} => {cluster_offset}");

        let mut cluster_bytes = vec![0; cluster_size];
        let max_sector = boot_sector
            .sectors_per_cluster()
            .min(file_size.div_ceil(SECTOR_SIZE));
        for sector_offset in 0..max_sector {
            let sector_start = sector_offset * SECTOR_SIZE;
            let sector_end = sector_start + SECTOR_SIZE;

            drive
                .read_blocks(
                    lba_start as usize + cluster_offset + sector_offset,
                    &mut cluster_bytes[sector_start..sector_end],
                )
                .map_err(|_| "failed to read cluster sector")?;
        }

        let read_size = cluster_size.min(file_size - byte_offset);
        bytes[byte_offset..byte_offset + read_size].copy_from_slice(&cluster_bytes[..read_size]);
        byte_offset += read_size;

        if byte_offset >= file_size {
            break;
        }

        // Go to the next cluster and continue reading.
        current_cluster = {
            // VFAT stores FAT entries the same as FAT16.
            let fat_offset = current_cluster * 2;
            let first_fat_sector = boot_sector.reserved_sector_count();
            let fat_sector = first_fat_sector + (fat_offset / SECTOR_SIZE);
            let entry_offset = fat_offset % SECTOR_SIZE;

            let mut sector = [0; SECTOR_SIZE];
            drive
                .read_blocks(lba_start as usize + fat_sector, &mut sector)
                .map_err(|_| "failed to read sector for FAT entry")?;

            u16::from_le_bytes([sector[entry_offset], sector[entry_offset + 1]]) as usize
        };

        if current_cluster == BAD_CLUSTER_INDEX as usize {
            return Err("encountered bad cluster");
        }
    }

    Ok(bytes)
}

#[repr(C)]
struct VfatBootSector {
    jmp_boot: [u8; 3],
    oem_name: [u8; 8],

    bytes_per_sector: [u8; 2],
    sectors_per_cluster: u8,
    reserved_sector_count: [u8; 2],
    fat_count: u8,
    root_entry_count: [u8; 2],
    sector_count_16: [u8; 2],
    media: u8,
    sectors_per_fat_16: [u8; 2],
    sectors_per_track: [u8; 2],
    head_count: [u8; 2],
    hidden_sector_count: [u8; 4],
    sector_count_32: [u8; 4],

    drive_number: u8,
    _reserved: u8,
    sig: u8,
    volume_serial_number: [u8; 4],
    volume_label: [u8; 11],
    fs_type_label: [u8; 8],

    boot_code: [u8; 448],
    signature: [u8; 2],
}

impl fmt::Debug for VfatBootSector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VfatBootSector")
            .field("oem_name", &self.oem_name())
            .field("volume_label", &self.volume_label())
            .field("fs_type_label", &self.fs_type_label())
            // Ratios.
            .field("bytes_per_sector", &self.bytes_per_sector())
            .field("sectors_per_cluster", &self.sectors_per_cluster())
            .field("sectors_per_track", &self.sectors_per_track())
            .field("sectors_per_fat", &self.sectors_per_fat())
            .field("entries_per_cluster", &self.entries_per_cluster())
            // Counts and offsets.
            .field("sector_count", &self.sector_count())
            .field("cluster_count", &self.cluster_count())
            .field("hidden_sector_count", &self.hidden_sector_count())
            .field("reserved_sector_count", &self.reserved_sector_count())
            .field("data_sector_count", &self.data_sector_count())
            .field("data_sector_offset", &self.data_sector_offset())
            .field("root_sector_offset", &self.root_sector_offset())
            .field("root_sector_count", &self.root_sector_count())
            .field("root_entry_count", &self.root_entry_count())
            .finish()
    }
}

impl VfatBootSector {
    pub fn oem_name(&self) -> &str {
        core::str::from_utf8(&self.oem_name).unwrap()
    }

    pub fn volume_label(&self) -> &str {
        core::str::from_utf8(&self.volume_label).unwrap()
    }

    pub fn fs_type_label(&self) -> &str {
        core::str::from_utf8(&self.fs_type_label).unwrap()
    }

    pub const fn is_fat32(&self) -> bool {
        self.sector_count_16() == 0
    }

    pub const fn bytes_per_sector(&self) -> usize {
        u16::from_le_bytes(self.bytes_per_sector) as usize
    }

    pub const fn sectors_per_cluster(&self) -> usize {
        self.sectors_per_cluster as usize
    }

    pub const fn entries_per_cluster(&self) -> usize {
        (self.bytes_per_sector() * self.sectors_per_cluster()) / size_of::<VfatDirectoryEntry>()
    }

    pub const fn sectors_per_track(&self) -> usize {
        u16::from_le_bytes(self.sectors_per_track) as usize
    }

    pub const fn sectors_per_fat(&self) -> usize {
        self.sectors_per_fat_16()
    }

    pub const fn sector_count(&self) -> usize {
        if self.is_fat32() {
            self.sector_count_32()
        } else {
            self.sector_count_16()
        }
    }

    const fn sector_count_16(&self) -> usize {
        u16::from_le_bytes(self.sector_count_16) as usize
    }

    const fn sector_count_32(&self) -> usize {
        u32::from_le_bytes(self.sector_count_32) as usize
    }

    pub const fn data_sector_offset(&self) -> usize {
        self.root_sector_offset() + self.root_sector_count()
    }

    pub const fn data_sector_count(&self) -> usize {
        self.sector_count() - self.data_sector_offset()
    }

    pub const fn hidden_sector_count(&self) -> usize {
        u32::from_le_bytes(self.hidden_sector_count) as usize
    }

    pub const fn reserved_sector_count(&self) -> usize {
        u16::from_le_bytes(self.reserved_sector_count) as usize
    }

    pub const fn root_entry_count(&self) -> usize {
        u16::from_le_bytes(self.root_entry_count) as usize
    }

    pub const fn root_sector_offset(&self) -> usize {
        self.reserved_sector_count() + (self.sectors_per_fat_16() * self.fat_count_16())
    }

    pub const fn root_sector_count(&self) -> usize {
        (self.root_entry_count() * 32 + self.bytes_per_sector() - 1) / self.bytes_per_sector()
    }

    pub const fn cluster_count(&self) -> usize {
        self.data_sector_count() / self.sectors_per_cluster()
    }

    const fn fat_count_16(&self) -> usize {
        self.fat_count as usize
    }

    const fn sectors_per_fat_16(&self) -> usize {
        u16::from_le_bytes(self.sectors_per_fat_16) as usize
    }
}

pub struct VfatFileSystem {
    drive: ata::Drive,
    lba_start: u32,
    boot_sector: VfatBootSector,
    cache: HashMap<String, DirectoryEntry>,
}

impl VfatFileSystem {
    fn new(
        mut drive: ata::Drive,
        boot_sector: VfatBootSector,
        lba_start: u32,
    ) -> Result<Self, &'static str> {
        debug!("Listing root directory entries...");

        let mut cache = HashMap::new();

        let cluster_offset = boot_sector.root_sector_offset();
        let mut current_lfn_buf = VecDeque::new();
        'read_sectors: for sector_offset in 0..boot_sector.root_sector_count() {
            let mut sector_bytes = [0; SECTOR_SIZE];
            drive
                .read_blocks(
                    lba_start as usize + cluster_offset + sector_offset,
                    &mut sector_bytes,
                )
                .map_err(|_| "failed to read root directory sector")?;

            let sector_entries: [VfatDirectoryEntry; ENTRIES_PER_SECTOR] =
                unsafe { core::mem::transmute(sector_bytes) };

            for entry in sector_entries {
                if entry.kind() == EntryKind::Null && entry.attr().is_none() {
                    break 'read_sectors; // No more entries to read.
                }

                if entry.lfn_index().is_some_and(|i| i >= 1) {
                    if let Some(name) = entry.long_file_name() {
                        current_lfn_buf.push_front(name);
                    }
                    continue;
                }

                if entry
                    .attr()
                    .is_some_and(|attr| matches!(attr, Attribute::Archive | Attribute::Directory))
                {
                    let file_name = if current_lfn_buf.len() > 0 {
                        current_lfn_buf.iter().fold(String::new(), |acc, s| acc + s)
                    } else {
                        entry.short_file_name().unwrap()
                    };
                    current_lfn_buf.clear();

                    let entry = DirectoryEntry {
                        index: entry.cluster_index(),
                        name: file_name,
                        size: entry.size(),
                    };

                    debug!("/{} ({} bytes) @ {}", entry.name, entry.size, entry.index);

                    cache.insert(format!("/{}", entry.name), entry);
                }
            }
        }

        Ok(Self {
            drive,
            lba_start,
            boot_sector,
            cache,
        })
    }
}

impl FileSystem for VfatFileSystem {
    fn list(&self, dir_path: &str) -> Result<Vec<String>, &'static str> {
        Ok(self
            .cache
            .iter()
            .filter_map(|(path, _entry)| path.starts_with(dir_path).then(|| path.to_string()))
            .collect())
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, &'static str> {
        let dir_entry = self
            .cache
            .get(path)
            .ok_or("TODO: Cache directory entries dynamically")?;

        read_file_bytes(
            &mut self.drive,
            self.lba_start,
            &self.boot_sector,
            dir_entry.index,
            dir_entry.size,
        )
    }
}

#[repr(C)]
pub struct VfatDirectoryEntry([u8; 32]);

impl fmt::Debug for VfatDirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VfatDirectoryEntry")
            .field("kind", &self.kind())
            .field("size", &self.size())
            .field("attr", &self.attr())
            .field("cluster_index", &self.cluster_index())
            .finish()
    }
}

impl VfatDirectoryEntry {
    pub const fn raw(&self) -> &[u8; 32] {
        &self.0
    }

    pub const fn attr(&self) -> Option<Attribute> {
        match self.raw()[11] {
            0x01 => Some(Attribute::ReadOnly),
            0x02 => Some(Attribute::Hidden),
            0x04 => Some(Attribute::System),
            0x08 => Some(Attribute::VolumeLabel),
            0x0F => Some(Attribute::LongFileName),
            0x10 => Some(Attribute::Directory),
            0x20 => Some(Attribute::Archive),
            0x40 => Some(Attribute::Device),

            _ => None,
        }
    }

    pub const fn kind(&self) -> EntryKind {
        let bytes = self.raw();
        match (bytes[0], bytes[10]) {
            (0x00, _) => EntryKind::Null,
            (0xE5, _) => EntryKind::Unused,
            (_, 0x0F) => EntryKind::LongFileName,

            _ => EntryKind::Data,
        }
    }

    pub const fn cluster_index(&self) -> usize {
        let bytes = self.raw();
        let hi = u16::from_le_bytes([bytes[20], bytes[21]]);
        let lo = u16::from_le_bytes([bytes[26], bytes[27]]);

        (((hi as u32) << 16) | (lo as u32)) as usize
    }

    pub const fn size(&self) -> usize {
        let bytes = self.raw();
        u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]) as usize
    }

    // https://wiki.osdev.org/FAT#Long_File_Names
    // https://en.wikipedia.org/wiki/Design_of_the_FAT_file_system#VFAT_long_file_names
    pub fn long_file_name(&self) -> Option<String> {
        if self.attr() != Some(Attribute::LongFileName) {
            return None;
        }

        let bytes = self.raw();
        let mut utf16_buf = Vec::new();

        // The first 5 characters.
        for i in (1..11).step_by(2) {
            if utf16_buf.iter().any(|ch| *ch == 0) {
                break;
            }

            utf16_buf.push(bytes[i] as u16 | bytes[i + 1] as u16);
        }

        // The next 6 characters.
        for i in (14..26).step_by(2) {
            if utf16_buf.iter().any(|ch| *ch == 0) {
                break;
            }

            utf16_buf.push(bytes[i] as u16 | bytes[i + 1] as u16);
        }

        // The final 2 characters.
        for i in (28..32).step_by(2) {
            if utf16_buf.iter().any(|ch| *ch == 0) {
                break;
            }

            utf16_buf.push(bytes[i] as u16 | bytes[i + 1] as u16);
        }

        Some(String::from_utf16_lossy(&utf16_buf).replace("\0", ""))
    }

    pub fn short_file_name(&self) -> Option<String> {
        match self.attr() {
            Some(attr) => match attr {
                Attribute::Archive | Attribute::Directory | Attribute::VolumeLabel => {}

                _ => return None,
            },
            None => return None,
        }

        Some(String::from_utf8_lossy(&self.raw()[0..11]).into_owned())
    }

    fn lfn_index(&self) -> Option<usize> {
        if self.attr() != Some(Attribute::LongFileName) {
            return None;
        }

        Some(self.raw()[0] as usize)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Attribute {
    ReadOnly = 0x01,
    Hidden = 0x02,
    System = 0x04,
    VolumeLabel = 0x08,
    LongFileName = 0x0F,
    Directory = 0x10,
    Archive = 0x20,
    Device = 0x40,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum EntryKind {
    #[default]
    Null,
    Unused,
    LongFileName,
    Data,
}
