//! # Advanced Technology Attachment (ATA)

use {
    crate::vfat,
    alloc::vec::Vec,
    boot_info::BootInfo,
    io::BlockReader as _,
    log::{info, trace, warn},
};


const SECTOR_SIZE: usize = 512;

pub fn init(boot_info: &BootInfo) {
    for mut drive in ata::enumerate_drives() {
        let partitions = get_drive_partitions(&mut drive).unwrap();
        info!(
            "ATA {:x}:{:x} | {drive}\n\
            \tpartitions: {partitions:?}",
            drive.bus, drive.id,
        );

        for partition in partitions {
            let Partition::Boot {
                fs_type,
                lba_start,
                lba_sector_count,
            } = partition;

            match fs_type {
                FSTYPE_VFAT => {
                    vfat::init(boot_info, &mut drive, lba_start, lba_sector_count);
                }
                other => {
                    warn!("Encountered unsupported filesystem type: {other}");
                }
            }
        }
    }
}



#[derive(Debug)]
pub enum Partition {
    Boot {
        fs_type: u8,
        lba_start: u32,
        lba_sector_count: u32,
    },
}

fn get_drive_partitions(drive: &mut ata::Drive) -> Result<Vec<Partition>, &'static str> {
    let mut buf = [0; SECTOR_SIZE];
    drive
        .read_blocks(0, &mut buf)
        .map_err(|_| "failed to read MBR sector")?;

    if &buf[510..512] != &[0x55, 0xAA] {
        return Err("MBR was not a valid boot sector");
    }

    let mut partitions = Vec::with_capacity(1);

    for entry_chunk in buf[446..510].chunks_exact(16) {
        if entry_chunk.iter().all(|byte| *byte == 0) {
            continue; // Entry is unused.
        }

        // SAFETY: This is just transmuting a 16-byte slice into a slightly more
        //         structured 16-byte slice.
        let entry = unsafe { &*(entry_chunk.as_ptr() as *const PartitionTableEntry) };
        trace!("Partition table entry: {entry:?}");

        if entry.drive_attrs != 0x80 {
            return Err("MBR partition table contained inactive partition");
        }

        let lba_start = u32::from_le_bytes(entry.lba_start_addr);
        let lba_sector_count = u32::from_le_bytes(entry.lba_sector_count);

        // TODO: Support drives with more than just the boot partition.
        if lba_start + lba_sector_count != drive.sector_count() as u32 {
            return Err("MBR partition table entry should cover the whole drive");
        }

        partitions.push(Partition::Boot {
            fs_type: entry.filesystem_type,
            lba_start,
            lba_sector_count,
        });
    }

    Ok(partitions)
}

const FSTYPE_VFAT: u8 = 0x06;

#[derive(Debug)]
#[repr(C)]
struct PartitionTableEntry {
    drive_attrs: u8,
    _chs_start_addr: [u8; 3],
    filesystem_type: u8,
    _chs_end_addr: [u8; 3],
    lba_start_addr: [u8; 4],
    lba_sector_count: [u8; 4],
}
