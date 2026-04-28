//! # Program Loading
//!
//! Types and functions used to dynamically load (and link) programs.

use {
    crate::{
        FileSystem,
        memory::{AddressSpace, KernelMapping},
    },
    alloc::{
        boxed::Box,
        collections::{btree_map::BTreeMap, btree_set::BTreeSet},
        string::{String, ToString as _},
        sync::{Arc, Weak},
        vec::Vec,
    },
    boot_info::BootInfo,
    core::{
        ops::Range,
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
        time::Duration,
    },
    elf::{
        ElfFile, ObjectFileType, SHF_ALLOC, SHF_EXECINSTR, SHF_TLS, SHF_WRITE, SectionData,
        SectionHeaderType, SymbolBinding, SymbolType,
    },
    hashbrown::HashMap,
    log::{debug, error, info, trace},
    memory_types::{Page, PageRange, PageTableFlags, VirtualAddress},
    spin_mutex::Mutex,
};

const AUTO_MAP_DEPENDENCIES: bool = false;

const FUNDAMENTAL_SYMBOLS: &[&str] = &[
    "memcmp",
    "memcpy",
    "memmove",
    "memset",
    "strlen",
    "__muldf3",
    "__mulsf3",
    "__divsf3",
    "__divdf3",
    "__udivti3",
    "__umodti3",
    "__floatdidf",
    "__floatdisf",
    "__eqsf2",
    "__gedf2",
    "__gesf2",
];

static LOADER: Loader = Loader::new();
static mut PROVIDER: Option<GlobalObjectProvider> = None;

/// Initialize the [global loader](global_loader).
pub fn init(boot_info: &BootInfo, fs: impl FileSystem + 'static) {
    unsafe {
        PROVIDER = Some(GlobalObjectProvider {
            fs: Mutex::new(Box::new(fs)),
        });
    }

    init_fundamental_symbols();

    // FIXME: The only reason this exists is because `core` relies on a symbol
    //        called `rust_begin_unwind`, which is only defined with the
    //        `#[panic_handler]` lang item. When it gets loaded, it expects that
    //        symbol to already be loaded. That's what this is doing.
    global_loader()
        .load_object(
            "example_dep",
            &AddressSpace::new("load_dep", None),
            // The actual value of this address doesn't matter.
            Page::containing_addr(VirtualAddress::new(0x3333_0000_0000)),
        )
        .unwrap();

    global_loader()
        .load_object(
            "time",
            &AddressSpace::new("load_time", None),
            // The actual value of this address doesn't matter.
            Page::containing_addr(VirtualAddress::new(0x3333_0000_0000)),
        )
        .unwrap();
    global_loader()
        .load_object(
            "framebuffer",
            &AddressSpace::new("load_framebuffer", None),
            // The actual value of this address doesn't matter.
            Page::containing_addr(VirtualAddress::new(0x3333_0000_0000)),
        )
        .unwrap();
    global_loader()
        .load_object(
            "input",
            &AddressSpace::new("load_input", None),
            // The actual value of this address doesn't matter.
            Page::containing_addr(VirtualAddress::new(0x3333_0000_0000)),
        )
        .unwrap();

    with_sym(
        "time",
        "MONOTONIC_PERIOD",
        |mono_period: &mut AtomicU64| unsafe {
            assert_eq!(mono_period.load(Ordering::SeqCst), 1);
            mono_period.store(crate::tsc::TSC_PERIOD, Ordering::SeqCst);
            assert_eq!(mono_period.load(Ordering::SeqCst), crate::tsc::TSC_PERIOD);
        },
    );
    with_sym(
        "framebuffer",
        "FRAMEBUFFER_ADDR",
        |fb_addr: &mut AtomicUsize| {
            assert_eq!(fb_addr.load(Ordering::SeqCst), 0);
            fb_addr.store(
                boot_info.display_info.framebuffer_addr as usize,
                Ordering::SeqCst,
            );
        },
    );
    with_sym(
        "framebuffer",
        "FRAMEBUFFER_SIZE",
        |fb_size: &mut AtomicUsize| {
            assert_eq!(fb_size.load(Ordering::SeqCst), 0);
            fb_size.store(boot_info.display_info.framebuffer_size, Ordering::SeqCst);
        },
    );
    with_sym(
        "framebuffer",
        "FRAMEBUFFER_WIDTH",
        |fb_width: &mut AtomicUsize| {
            assert_eq!(fb_width.load(Ordering::SeqCst), 0);
            fb_width.store(boot_info.display_info.stride as usize, Ordering::SeqCst);
        },
    );
    with_sym(
        "framebuffer",
        "FRAMEBUFFER_HEIGHT",
        |fb_height: &mut AtomicUsize| {
            assert_eq!(fb_height.load(Ordering::SeqCst), 0);
            fb_height.store(boot_info.display_info.height as usize, Ordering::SeqCst);
        },
    );

    // global_loader().dump_info();
}

fn with_sym<T, F>(section_prefix: &str, section_suffix: &str, op: F)
where
    T: Sized,
    F: FnOnce(&mut T),
{
    let sym = global_loader()
        .get_section(section_prefix, section_suffix)
        .unwrap();
    let value = sym.upgrade().unwrap();
    let mut mapping = value.mapping.lock();
    op(unsafe { mapping.as_mut::<T>(value.mapping_offset) });
}

fn init_fundamental_symbols() {
    let dummy_addr_space = AddressSpace::new("load_fundamental", None);

    global_loader()
        .load_object(
            "compiler_builtins",
            &dummy_addr_space,
            // The actual value of this address doesn't matter.
            Page::containing_addr(VirtualAddress::new(0x2222_0000_0000)),
        )
        .unwrap();

    for name in FUNDAMENTAL_SYMBOLS {
        let Some(section) = global_loader().get_section("compiler_builtins", name) else {
            panic!("Couldn't find section for fundamental symbol `{name}`");
        };
        global_loader().add_alias_to_section(name, section);
    }
}

/// Get a reference to the global object loader.
pub fn global_loader<'a>() -> &'a Loader {
    &LOADER
}

/// Get a reference to the [`GlobalObjectProvider`].
pub fn global_object_provider<'a>() -> &'a GlobalObjectProvider {
    unsafe {
        PROVIDER
            .as_ref()
            .expect("global object provider should be initialized")
    }
}

/// The global object provider.
///
/// Internally, this is just a [`FileSystem`] trait object wrapped in a
/// [`Mutex`].
pub struct GlobalObjectProvider {
    fs: Mutex<Box<dyn FileSystem>>,
}

impl<'a> ObjectProvider for &'a GlobalObjectProvider {
    fn list_objects(&self, prefix: &str) -> Result<Vec<String>, &'static str> {
        self.fs.lock().list(&format!("/{prefix}"))
    }

    fn read_object(&self, name: &str) -> Result<Vec<u8>, &'static str> {
        if !name.starts_with("/") {
            let path = self
                .list_objects(name)?
                .into_iter()
                .find(|object_name| object_name == &format!("/{name}.o"))
                .ok_or("no object found")?;

            self.fs.lock().read(&path)
        } else {
            self.fs.lock().read(name)
        }
    }
}

/// A set of loaded [objects](LoadedObject) and [sections](LoadedSection).
#[derive(Debug)]
pub struct Loader {
    objects: Mutex<HashMap<Arc<str>, Arc<Mutex<LoadedObject>>, rustc_hash::FxBuildHasher>>,
    sections: Mutex<HashMap<Arc<str>, Weak<LoadedSection>, rustc_hash::FxBuildHasher>>,
    sections_by_addr: Mutex<BTreeMap<(VirtualAddress, usize), Weak<LoadedSection>>>,
}

/// An object that has been loaded into memory.
#[derive(Debug)]
pub struct LoadedObject {
    /// The demangled name of this object.
    pub name: Arc<str>,
    /// The sections that have been loaded into memory for this object.
    pub sections: HashMap<usize, Arc<LoadedSection>>,
    /// A set of section indices representing the global sections of this
    /// object. They can be used as keys for [`self.sections`](Self::sections).
    pub global_sections: BTreeSet<usize>,
    /// A set of section indices representing the data sections of this object.
    /// They can be used as keys for [`self.sections`](Self::sections).
    pub data_sections: BTreeSet<usize>,
    /// A set of section indices representing the thread-local storage (TLS)
    /// sections of this object. They can be used as keys for
    /// [`self.sections`](Self::sections).
    pub tls_sections: BTreeSet<usize>,
    /// Objects this object depends on.
    pub dependencies: Vec<Weak<Mutex<LoadedObject>>>,
    pub executable_mapping: Option<Arc<Mutex<KernelMapping>>>,
    pub read_only_mapping: Option<Arc<Mutex<KernelMapping>>>,
    pub read_write_mapping: Option<Arc<Mutex<KernelMapping>>>,
}

/// An object section that has been loaded into memory.
#[derive(Debug)]
pub struct LoadedSection {
    /// The demangled name of this section.
    pub name: Arc<str>,
    /// The type of this section (`.text`, `.data`, etc.).
    pub kind: SectionKind,
    /// Whether this section is global (public).
    pub global: bool,
    /// The size of this section in bytes.
    pub size: usize,
    /// The memory address of this section.
    pub addr: VirtualAddress,
    /// A reference to the mapping that contains this section's data.
    pub mapping: Arc<Mutex<KernelMapping>>,
    /// The offset into [`self.mapping`](Self::mapping) at which this section's
    /// data starts.
    pub mapping_offset: usize,
    /// The object that contains this section.
    pub owner: Weak<Mutex<LoadedObject>>,
}

/// The type of a [`LoadedSection`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SectionKind {
    /// Executable code.
    Text,
    /// Immutable program data.
    Rodata,
    /// Mutable program data.
    Data,
    /// Uninitialized program data.
    Bss,
    /// Thread-local data.
    TlsData,
    TlsBss,
    GccExceptTable,
    /// Exception handling (unwind) information.
    EhFrame,
}

impl SectionKind {
    pub fn name(&self) -> &'static str {
        match self {
            SectionKind::Text => ".text",
            SectionKind::Rodata => ".rodata",
            SectionKind::Data => ".data",
            SectionKind::Bss => ".bss",
            SectionKind::TlsData => ".tdata",
            SectionKind::TlsBss => ".tbss",
            SectionKind::GccExceptTable => ".gcc_except_table",
            SectionKind::EhFrame => ".eh_frame",
        }
    }
}

/// Something capable of reading object data and listing available objects.
pub trait ObjectProvider {
    /// Get a list of object names that match the given prefix.
    fn list_objects(&self, prefix: &str) -> Result<Vec<String>, &'static str>;
    /// Read the bytes of the object with the given name.
    fn read_object(&self, name: &str) -> Result<Vec<u8>, &'static str>;
}

impl Loader {
    /// Create an empty `Loader` without any loaded [objects](LoadedObject) or
    /// [sections](LoadedSection).
    pub const fn new() -> Self {
        Self {
            objects: Mutex::new(HashMap::with_hasher(rustc_hash::FxBuildHasher)),
            sections: Mutex::new(HashMap::with_hasher(rustc_hash::FxBuildHasher)),
            sections_by_addr: Mutex::new(BTreeMap::new()),
        }
    }

    /// Dump debug information to the logger.
    pub fn dump_info(&self) {
        let objects = self.objects.lock();
        // let sections = self.sections.lock();

        debug!(
            "--- OBJECTS ---\n{}",
            objects
                .iter()
                .map(|(name, object)| {
                    let object = object.lock();
                    let section_count = object.sections.len();
                    format!(
                        "    {name}:{}\n",
                        if section_count > 20 {
                            format!("\n        {section_count} sections")
                        } else {
                            object
                                .sections
                                .iter()
                                .map(|(index, section)| {
                                    format!(
                                        "\n    {index:>4} | {:#x} | {:>10} | {}",
                                        section.addr,
                                        section.kind.name(),
                                        &section.name[..section.name.len().min(50)],
                                    )
                                })
                                .collect::<String>()
                        },
                    )
                })
                .collect::<String>(),
        );
        // debug!(
        //     "--- SECTIONS ---\n{}",
        //     sections
        //         .iter()
        //         .filter(|(name, _section)| !name.starts_with("<")
        //             && !name.contains("core[")
        //             && !name.contains("compiler_builtins[")
        //             && !name.contains("alloc[")
        //             && !name.starts_with("anon."))
        //         .map(|(name, section)| format!(
        //             "    {}: {} ref(s)\n",
        //             &name[..name.len().min(60)],
        //             section.weak_count(),
        //         ))
        //         .collect::<String>(),
        // );
    }

    /// Get the [object](LoadedObject) with the given name.
    pub fn get_object(&self, name: &str) -> Option<Weak<Mutex<LoadedObject>>> {
        self.objects.lock().get(name).map(Arc::downgrade)
    }

    /// Get the first [section](LoadedSection) that starts with the given prefix
    /// and ends with the given suffix.
    pub fn get_section(&self, prefix: &str, suffix: &str) -> Option<Weak<LoadedSection>> {
        self.sections
            .lock()
            .iter()
            .find(|(name, _section)| name.starts_with(prefix) && name.ends_with(suffix))
            .map(|(_name, section)| section.clone())
    }

    /// Get the first text [section](LoadedSection) that starts with the given
    /// prefix and ends with the given suffix.
    pub fn get_text_section(&self, prefix: &str, suffix: &str) -> Option<Weak<LoadedSection>> {
        self.sections
            .lock()
            .iter()
            .find(|(name, section)| {
                section
                    .upgrade()
                    .is_some_and(|section| matches!(section.kind, SectionKind::Text))
                    && name.starts_with(prefix)
                    && name.ends_with(suffix)
            })
            .map(|(_name, section)| section.clone())
    }

    /// Get the first [section](LoadedSection) that contains the given address.
    pub fn get_section_for_addr(&self, addr: VirtualAddress) -> Option<Weak<LoadedSection>> {
        self.sections_by_addr
            .lock()
            .iter()
            .find(|((section_addr, section_size), _section)| {
                &addr >= section_addr && addr < *section_addr + *section_size
            })
            .map(|(_range, section)| section.clone())
    }

    fn get_or_load_section(
        &self,
        name: &str,
        for_object: &LoadedObject,
        address_space: &AddressSpace,
        start_page: &mut Page,
    ) -> Result<Weak<LoadedSection>, &'static str> {
        if let Some(section) = self.sections.lock().get(name) {
            return Ok(section.clone());
        }

        for object_name in crate_names_in_symbol(name) {
            // Skip already loaded objects.
            if self.get_object(&object_name).is_some() {
                continue;
            }

            trace!(
                "Loading object `{object_name}` as a dependency of `{}` for symbol `{name}`",
                for_object.name,
            );

            self.load_object_impl(
                &object_name,
                &global_object_provider().read_object(&object_name)?,
                address_space,
                start_page,
            )?;

            if let Some(section) = self.sections.lock().get(name) {
                return Ok(section.clone());
            }
        }

        // error!("Failed to load `{name}` for `{}`", for_object.name);

        Err("section not found")
    }

    fn add_alias_to_section(&self, name: &str, section: Weak<LoadedSection>) {
        self.sections.lock().insert(name.into(), section);
    }

    /// Load an object into memory.
    ///
    /// Internally, this method uses the [`GlobalObjectProvider`] to read
    /// object data.
    ///
    /// ## Arguments
    ///
    /// - `object_name`, the name of the object to be loaded.
    /// - `address_space`, the [`AddressSpace`] to load the object into.
    /// - `start_page`, the starting page within `address_space` at which the
    ///   object (and its dependencies) will be loaded.
    pub fn load_object(
        &self,
        object_name: &str,
        address_space: &AddressSpace,
        mut start_page: Page,
    ) -> Result<Arc<Mutex<LoadedObject>>, &'static str> {
        info!("Loading `{object_name}`...");
        let object_bytes = global_object_provider().read_object(object_name)?;
        self.load_object_impl(object_name, &object_bytes, address_space, &mut start_page)
    }

    fn load_object_impl(
        &self,
        object_name: &str,
        object_bytes: &[u8],
        address_space: &AddressSpace,
        start_page: &mut Page,
    ) -> Result<Arc<Mutex<LoadedObject>>, &'static str> {
        let mut mappings = BTreeSet::new();
        let (object, elf_file) = self.load_object_sections(
            object_name,
            object_bytes,
            address_space,
            start_page,
            &mut mappings,
        )?;
        self.add_sections(object.lock().sections.values());
        self.objects
            .lock()
            .insert(object_name.into(), Arc::clone(&object));
        self.relocate_object_sections(
            &elf_file,
            &object,
            address_space,
            start_page,
            &mut mappings,
        )?;

        Ok(object)
    }

    fn load_object_sections<'obj>(
        &self,
        object_name: &'obj str,
        object_bytes: &'obj [u8],
        address_space: &AddressSpace,
        start_page: &mut Page,
        mappings: &mut BTreeSet<VirtualAddress>,
    ) -> Result<(Arc<Mutex<LoadedObject>>, ElfFile<'obj>), &'static str> {
        let elf_file = ElfFile::new(object_bytes)?;
        if elf_file.header.get_type() != ObjectFileType::Relocatable {
            return Err("not a relocatable ELF file");
        }

        let SectionMappings {
            executable: executable_mapping,
            read_only: read_only_mapping,
            read_write: read_write_mapping,
        } = allocate_section_mappings(object_name, &elf_file)?;

        // Map loaded sections into the object's address space.
        if let Some(mapping) = &executable_mapping {
            let pages = PageRange::from_start_len(*start_page, mapping.pages.len());
            mappings.insert(mapping.addr);
            mapping
                .map_into(
                    address_space,
                    pages,
                    PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE,
                )
                .unwrap();
            *start_page = pages.end;
        }
        if let Some(mapping) = &read_only_mapping {
            let pages = PageRange::from_start_len(*start_page, mapping.pages.len());
            mappings.insert(mapping.addr);
            mapping
                .map_into(
                    address_space,
                    pages,
                    PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE,
                )
                .unwrap();
            *start_page = pages.end;
        }
        if let Some(mapping) = &read_write_mapping {
            let pages = PageRange::from_start_len(*start_page, mapping.pages.len());
            mappings.insert(mapping.addr);
            mapping
                .map_into(
                    address_space,
                    pages,
                    PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE,
                )
                .unwrap();
            *start_page = pages.end;
        }

        let executable_mapping = executable_mapping.map(|mapping| Arc::new(Mutex::new(mapping)));
        let read_only_mapping = read_only_mapping.map(|mapping| Arc::new(Mutex::new(mapping)));
        let read_write_mapping = read_write_mapping.map(|mapping| Arc::new(Mutex::new(mapping)));

        // The `.text` sections always come at the beginning, so we can get the byte
        // range without needing to know the offset.
        if let Some(executable_mapping) = &executable_mapping {
            let mut executable_map_lock = executable_mapping.lock();
            let text_size = executable_map_lock.size();
            let slice = elf_file.input.get(..text_size).ok_or_else(|| {
                error!("End of last `.text` section ({text_size}) was miscalculated to be beyond ELF file bounds ({})", elf_file.input.len());
                "end of last `.text` section was miscalculated to be beyond ELF file bounds"
            })?;

            executable_map_lock
                .as_slice_mut(0, text_size)
                .copy_from_slice(slice);
        }

        let object = Arc::new(Mutex::new(LoadedObject {
            name: rustc_demangle::demangle(object_name).to_string().into(),
            sections: HashMap::new(),
            global_sections: BTreeSet::new(),
            data_sections: BTreeSet::new(),
            tls_sections: BTreeSet::new(),
            dependencies: Vec::new(),
            executable_mapping: executable_mapping.clone(),
            read_only_mapping: read_only_mapping.clone(),
            read_write_mapping: read_write_mapping.clone(),
        }));

        let mut loaded_sections: HashMap<usize, Arc<LoadedSection>> = HashMap::new();
        let mut data_sections: BTreeSet<usize> = BTreeSet::new();
        let mut tls_sections: BTreeSet<usize> = BTreeSet::new();
        let global_sections: BTreeSet<usize> = {
            let symbol_table = elf_file.get_symbol_table()?;
            let mut globals: BTreeSet<usize> = BTreeSet::new();
            for entry in symbol_table.iter() {
                if entry.get_binding() == Ok(SymbolBinding::Global) {
                    match entry.get_type() {
                        Ok(SymbolType::Func | SymbolType::Object | SymbolType::Tls) => {
                            globals.insert(entry.shndx() as usize);
                        }
                        _ => continue,
                    }
                }
            }

            globals
        };

        let mut rodata_offset = 0;
        let mut data_offset = 0;

        for (section_index, section) in elf_file.section_iter().enumerate() {
            let section_flags = section.flags();

            // Skip non-allocated sections.
            if section_flags & SHF_ALLOC == 0 {
                continue;
            }

            // If the current section is zero-sized, it's a reference to the next section.
            // So, we just use the next section's information (size, align, etc.) with the
            // current section's name.
            let section_name = section.get_name(&elf_file)?;
            let section = if section.size() == 0 {
                // If the next section has the same offset as the current one, use it instead of
                // the current one.
                match elf_file.get_section_header((section_index + 1) as u16) {
                    Ok(next_section) => {
                        if next_section.offset() == section.offset() {
                            next_section
                        } else {
                            section
                        }
                    }
                    _ => {
                        return Err("couldn't get the section following a zero-sized section");
                    }
                }
            } else {
                section
            };

            let section_size = section.size() as usize;
            let section_align = section.align() as usize;

            let is_write = section_flags & SHF_WRITE == SHF_WRITE;
            let is_exec = section_flags & SHF_EXECINSTR == SHF_EXECINSTR;
            let is_tls = section_flags & SHF_TLS == SHF_TLS;

            macro_rules! symbol_name_after_prefix {
                ($sec_name:ident, $prefix:literal) => {
                    if let Some(name) = $sec_name.get($prefix.len()..) {
                        name
                    } else {
                        // Ignore placeholder sections.
                        match $sec_name {
                            ".text" | ".rodata" | ".data" | ".bss" => continue,
                            _ => {
                                return Err(concat!(
                                    "failed to get the ",
                                    $prefix,
                                    " section's name after '",
                                    $prefix,
                                    "'"
                                ));
                            }
                        }
                    }
                };
            }

            // .text
            if is_exec && !is_write {
                let Some(executable_mapping) = &executable_mapping else {
                    continue;
                };

                let is_global = global_sections.contains(&section_index);
                let mut name = symbol_name_after_prefix!(section_name, ".text.");
                if name.starts_with(".") {
                    name = name.strip_prefix(".").unwrap();
                }
                let name = if is_global && name.starts_with("unlikely.") {
                    name.get("unlikely.".len()..)
                        .ok_or("failed to get `.text.unlikely.` section's name")?
                } else {
                    name
                };

                // We already copied the content of all `.text` sections above, so here we just
                // record the metadata into a new `LoadedSection` object.
                let text_offset = section.offset() as usize;
                let section_addr = executable_mapping.lock().addr + text_offset;

                loaded_sections.insert(
                    section_index,
                    Arc::new(LoadedSection {
                        name: rustc_demangle::demangle(name).to_string().into(),
                        kind: SectionKind::Text,
                        size: section_size,
                        addr: section_addr,
                        global: is_global,
                        mapping: Arc::clone(&executable_mapping),
                        mapping_offset: text_offset,
                        owner: Arc::downgrade(&object),
                    }),
                );
            }
            // .tdata/.tbss
            else if is_tls {
                let Some(read_only_mapping) = &read_only_mapping else {
                    continue;
                };
                let mut read_only_map_lock = read_only_mapping.lock();

                // check if this TLS section is .bss or .data
                let is_bss = section.get_type() == Ok(SectionHeaderType::NoBits);
                let name = if is_bss {
                    symbol_name_after_prefix!(section_name, ".tbss.")
                } else {
                    symbol_name_after_prefix!(section_name, ".tdata.")
                };

                let (mapping_offset, kind) = if is_bss {
                    // Offset is irrelevant here.
                    (usize::MAX, SectionKind::TlsBss)
                } else {
                    let slice = read_only_map_lock.as_slice_mut(rodata_offset, section_size);
                    match section.get_data(&elf_file) {
                        Ok(SectionData::Undefined(sec_data)) => slice.copy_from_slice(sec_data),
                        _ => {
                            return Err("couldn't get data for `.tdata` section");
                        }
                    };

                    (rodata_offset, SectionKind::TlsData)
                };

                let tls_section = Arc::new(LoadedSection {
                    name: rustc_demangle::demangle(name).to_string().into(),
                    kind,
                    size: section_size,
                    addr: VirtualAddress::new(0), // See below.
                    global: global_sections.contains(&section_index),
                    mapping: Arc::clone(&read_only_mapping),
                    mapping_offset,
                    owner: Arc::downgrade(&object),
                });

                // This should initialize a TLS area and set the section's address.
                if true {
                    return Err("TODO: TLS section initialization");
                }

                loaded_sections.insert(section_index, tls_section);
                tls_sections.insert(section_index);

                rodata_offset += section_size.next_multiple_of(section_align);
            }
            // .data/.bss
            else if is_write {
                let Some(read_write_mapping) = &read_write_mapping else {
                    continue;
                };
                let mut read_write_map_lock = read_write_mapping.lock();

                let is_bss = section.get_type() == Ok(SectionHeaderType::NoBits);
                let mut name = if is_bss {
                    symbol_name_after_prefix!(section_name, ".bss.")
                } else {
                    symbol_name_after_prefix!(section_name, ".data.")
                };
                if name.starts_with(".") {
                    name = name.strip_prefix(".").unwrap();
                }

                assert!(data_offset < read_write_map_lock.size());
                let section_addr = read_write_map_lock.addr + data_offset;

                let slice = read_write_map_lock.as_slice_mut(data_offset, section_size);
                match section.get_data(&elf_file) {
                    Ok(SectionData::Undefined(sec_data)) => slice.copy_from_slice(sec_data),
                    Ok(SectionData::Empty) => slice.fill(0),
                    _ => {
                        return Err("couldn't get data for `.data` section");
                    }
                }

                loaded_sections.insert(
                    section_index,
                    Arc::new(LoadedSection {
                        name: rustc_demangle::demangle(name).to_string().into(),
                        kind: if is_bss {
                            SectionKind::Bss
                        } else {
                            SectionKind::Data
                        },
                        size: section_size,
                        addr: section_addr,
                        global: global_sections.contains(&section_index),
                        mapping: Arc::clone(&read_write_mapping),
                        mapping_offset: data_offset,
                        owner: Arc::downgrade(&object),
                    }),
                );
                data_sections.insert(section_index);

                data_offset += section_size.next_multiple_of(section_align);
            }
            // .rodata
            else if section_name.starts_with(".rodata") {
                let Some(read_only_mapping) = &read_only_mapping else {
                    continue;
                };
                let mut read_only_map_lock = read_only_mapping.lock();

                let name = symbol_name_after_prefix!(section_name, ".rodata.");

                assert!(rodata_offset < read_only_map_lock.size());
                let section_addr = read_only_map_lock.addr + rodata_offset;

                let slice = read_only_map_lock.as_slice_mut(rodata_offset, section_size);
                match section.get_data(&elf_file) {
                    Ok(SectionData::Undefined(sec_data)) => slice.copy_from_slice(sec_data),
                    Ok(SectionData::Empty) => slice.fill(0),
                    _ => {
                        return Err("couldn't get data for `.rodata` section");
                    }
                }

                loaded_sections.insert(
                    section_index,
                    Arc::new(LoadedSection {
                        name: rustc_demangle::demangle(name).to_string().into(),
                        kind: SectionKind::Rodata,
                        size: section_size,
                        addr: section_addr,
                        global: global_sections.contains(&section_index),
                        mapping: Arc::clone(&read_only_mapping),
                        mapping_offset: rodata_offset,
                        owner: Arc::downgrade(&object),
                    }),
                );

                rodata_offset += section_size.next_multiple_of(section_align);
            }
            // .lrodata
            else if section_name.starts_with(".lrodata") {
                let Some(read_only_mapping) = &read_only_mapping else {
                    continue;
                };
                let mut read_only_map_lock = read_only_mapping.lock();

                let mut name = symbol_name_after_prefix!(section_name, ".lrodata.");
                if name.starts_with(".") {
                    name = name.strip_prefix(".").unwrap();
                }

                assert!(rodata_offset < read_only_map_lock.size());
                let section_addr = read_only_map_lock.addr + rodata_offset;

                let slice = read_only_map_lock.as_slice_mut(rodata_offset, section_size);
                match section.get_data(&elf_file) {
                    Ok(SectionData::Undefined(sec_data)) => slice.copy_from_slice(sec_data),
                    Ok(SectionData::Empty) => slice.fill(0),
                    _ => {
                        return Err("couldn't get data for `.lrodata` section");
                    }
                }

                loaded_sections.insert(
                    section_index,
                    Arc::new(LoadedSection {
                        name: rustc_demangle::demangle(name).to_string().into(),
                        kind: SectionKind::Rodata,
                        size: section_size,
                        addr: section_addr,
                        global: global_sections.contains(&section_index),
                        mapping: Arc::clone(&read_only_mapping),
                        mapping_offset: rodata_offset,
                        owner: Arc::downgrade(&object),
                    }),
                );

                rodata_offset += section_size.next_multiple_of(section_align);
            }
            // .gcc_except_table
            else if section_name.starts_with(".gcc_except_table") {
                let Some(read_only_mapping) = &read_only_mapping else {
                    continue;
                };
                let mut read_only_map_lock = read_only_mapping.lock();

                assert!(rodata_offset < read_only_map_lock.size());
                let section_addr = read_only_map_lock.addr + rodata_offset;

                let slice = read_only_map_lock.as_slice_mut(rodata_offset, section_size);
                match section.get_data(&elf_file) {
                    Ok(SectionData::Undefined(sec_data)) => slice.copy_from_slice(sec_data),
                    Ok(SectionData::Empty) => slice.fill(0),
                    _ => {
                        return Err("couldn't get data for `.gcc_except_table` section");
                    }
                }

                let kind = SectionKind::GccExceptTable;
                loaded_sections.insert(
                    section_index,
                    Arc::new(LoadedSection {
                        name: kind.name().into(), // Ignore actual table name.
                        kind,
                        size: section_size,
                        addr: section_addr,
                        global: false,
                        mapping: Arc::clone(&read_only_mapping),
                        mapping_offset: rodata_offset,
                        owner: Arc::downgrade(&object),
                    }),
                );

                rodata_offset += section_size.next_multiple_of(section_align);
            }
            // .eh_frame
            else if section_name == ".eh_frame" {
                let Some(read_only_mapping) = &read_only_mapping else {
                    continue;
                };
                let mut read_only_map_lock = read_only_mapping.lock();

                assert!(rodata_offset < read_only_map_lock.size());
                let section_addr = read_only_map_lock.addr + rodata_offset;

                let slice = read_only_map_lock.as_slice_mut(rodata_offset, section_size);
                match section.get_data(&elf_file) {
                    Ok(SectionData::Undefined(sec_data)) => slice.copy_from_slice(sec_data),
                    Ok(SectionData::Empty) => slice.fill(0),
                    _ => {
                        return Err("couldn't get data for `.eh_frame` section");
                    }
                }

                let kind = SectionKind::EhFrame;
                loaded_sections.insert(
                    section_index,
                    Arc::new(LoadedSection {
                        name: kind.name().into(), // Ignore actual table name.
                        kind,
                        size: section_size,
                        addr: section_addr,
                        global: false,
                        mapping: Arc::clone(&read_only_mapping),
                        mapping_offset: rodata_offset,
                        owner: Arc::downgrade(&object),
                    }),
                );

                rodata_offset += section_size.next_multiple_of(section_align);
            }
            // Unhandled section.
            else {
                error!("Encountered unhandled section: `{section_name}`");
                return Err("encountered unhandled section");
            }
        }

        {
            let mut object_lock = object.lock();
            object_lock.sections = loaded_sections;
            object_lock.global_sections = global_sections;
            object_lock.data_sections = data_sections;
            object_lock.tls_sections = tls_sections;
        }

        Ok((object, elf_file))
    }

    fn relocate_object_sections(
        &self,
        elf_file: &ElfFile,
        object: &Arc<Mutex<LoadedObject>>,
        address_space: &AddressSpace,
        start_page: &mut Page,
        mappings: &mut BTreeSet<VirtualAddress>,
    ) -> Result<(), &'static str> {
        let mut object = object.lock();
        let symbol_table = elf_file.get_symbol_table()?;

        for section in elf_file.section_iter().filter(|section| {
            section.get_type() == Ok(SectionHeaderType::Rela) && section.size() != 0
        }) {
            let rela_array = match section.get_data(elf_file) {
                Ok(SectionData::Rela(rela_arr)) => rela_arr,
                _ => {
                    return Err("found `rela` section that wasn't able to be parsed");
                }
            };

            let target_section_index = section.info() as usize;
            let target_section = object
                .sections
                .get(&target_section_index)
                .cloned()
                .ok_or("target section was not loaded for `rela` section")?;

            {
                let mut target_section_mapping = target_section.mapping.lock();
                let target_slice = target_section_mapping
                    .as_slice_mut(0, target_section.mapping_offset + target_section.size);

                for rela_entry in rela_array {
                    let source_entry = &symbol_table[rela_entry.get_symbol_table_index() as usize];
                    let source_index = source_entry.shndx() as usize;
                    let source_value = source_entry.value() as usize;

                    let source_section = match object.sections.get(&source_index) {
                        Some(section) => section.clone(),
                        None => {
                            let name = source_entry
                                .get_name(&elf_file)
                                .map_err(|_| "couldn't get name of source section")?;
                            let name = if name.starts_with(".data.rel.ro.") {
                                name.get(".data.rel.ro.".len()..)
                                    .ok_or("couldn't get name of `.data.rel.ro.` section")?
                            } else {
                                name
                            };

                            let demangled_name = rustc_demangle::demangle(name).to_string();

                            let section = match self.get_or_load_section(
                                &demangled_name,
                                &object,
                                address_space,
                                start_page,
                            ) {
                                Ok(section) => section.upgrade().ok_or(
                                    "couldn't upgrade section reference for relocation entry",
                                ),
                                Err(error) => {
                                    // HACK: For now, fully relocating libcore isn't entirely
                                    //       possible because many of the math symbols just aren't
                                    //       supported yet. Remove this when they are.
                                    if &*object.name == "core" {
                                        let start = time::now();
                                        while time::now().duration_since(start)
                                            < Duration::from_millis(1)
                                        {
                                            core::hint::spin_loop();
                                        }
                                        continue;
                                    } else {
                                        error!(
                                            "Couldn't get relocation section for `{}` <- `{}`: \
                                            {error}",
                                            target_section.name, demangled_name,
                                        );
                                        return Err(error);
                                    }
                                }
                            }?;

                            // At this point, we know `section` is some external dependency (i.e.
                            // `section.owner` != `object`).
                            if AUTO_MAP_DEPENDENCIES {
                                map_dependency(&mut object, &section, address_space, mappings);
                            }

                            section
                        }
                    };

                    let target_offset =
                        target_section.mapping_offset + rela_entry.get_offset() as usize;

                    write_relocation(
                        rela_entry,
                        target_slice,
                        target_offset,
                        source_section.addr + source_value,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn add_sections<'a, I>(&self, sections: I) -> usize
    where
        I: IntoIterator<Item = &'a Arc<LoadedSection>>,
    {
        let mut map = self.sections.lock();
        let mut range_map = self.sections_by_addr.lock();
        let mut added_count = 0;
        for new_section in sections.into_iter() {
            range_map.insert(
                (new_section.addr, new_section.size),
                Arc::downgrade(new_section),
            );
            if new_section.global {
                if let Some(old_section) =
                    map.insert(new_section.name.clone(), Arc::downgrade(new_section))
                {
                    let old_section = old_section.upgrade().unwrap();
                    debug!(
                        "Moved `{}` from {:x} to {:x}",
                        old_section.name, old_section.addr, new_section.addr,
                    );
                } else {
                    added_count += 1;
                }
            }
        }

        added_count
    }
}

/// Map the section's dependencies into the object's address space.
fn map_dependency(
    object: &mut LoadedObject,
    section: &Arc<LoadedSection>,
    address_space: &AddressSpace,
    mappings: &mut BTreeSet<VirtualAddress>,
) {
    object.dependencies.push(section.owner.clone());

    let owner = section
        .owner
        .upgrade()
        .expect("should be able to get the source section owner");
    let mut owner_lock = owner.lock();

    map_dependency_sections(object, &mut owner_lock, address_space, mappings);
}

fn map_dependency_sections(
    object: &mut LoadedObject,
    dependency: &mut LoadedObject,
    address_space: &AddressSpace,
    mappings: &mut BTreeSet<VirtualAddress>,
) {
    assert_ne!(object.name, dependency.name);

    // An error in `map_into` just means it was already mapped. For kernel
    // processes, this is fine.

    // TODO: This should check if the address space has inherited the kernel address
    //       space.
    if let Some(exec_mapping) = dependency.executable_mapping.as_ref() {
        let exec_lock = exec_mapping.lock();
        if !mappings.contains(&exec_lock.addr) {
            let pages = exec_lock.pages;
            let flags = exec_lock.flags;
            _ = exec_lock.map_into(address_space, pages, flags);
            mappings.insert(exec_lock.addr);
        }
    }
    if let Some(ro_mapping) = dependency.read_only_mapping.as_ref() {
        let ro_lock = ro_mapping.lock();
        if !mappings.contains(&ro_lock.addr) {
            let pages = ro_lock.pages;
            let flags = ro_lock.flags;
            _ = ro_lock.map_into(address_space, pages, flags);
            mappings.insert(ro_lock.addr);
        }
    }
    if let Some(rw_mapping) = dependency.read_write_mapping.as_ref() {
        let rw_lock = rw_mapping.lock();
        if !mappings.contains(&rw_lock.addr) {
            let pages = rw_lock.pages;
            let flags = rw_lock.flags;
            _ = rw_lock.map_into(address_space, pages, flags);
            mappings.insert(rw_lock.addr);
        }
    }

    for secondary_dep in dependency.dependencies.iter() {
        map_dependency_sections(
            object,
            &mut secondary_dep.upgrade().unwrap().lock(),
            address_space,
            mappings,
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn write_relocation(
    relocation_entry: &elf::Rela,
    target_slice: &mut [u8],
    target_offset: usize,
    source_addr: VirtualAddress,
) -> Result<(), &'static str> {
    // https://docs.rs/goblin/latest/src/goblin/elf/constants_relocation.rs.html
    const R_X86_64_64: u32 = 1;
    const R_X86_64_PC32: u32 = 2;
    const R_X86_64_PLT32: u32 = 4;
    const R_X86_64_32: u32 = 10;
    const R_X86_64_32S: u32 = 11;
    const R_X86_64_PC64: u32 = 24;

    // trace!(
    //     "REL({}): {source_addr:#x} | {:#p}, {target_offset:#x}",
    //     relocation_entry.get_type(),
    //     target_slice.as_ptr(),
    // );

    let source_addr = source_addr.to_raw() as u64;
    match relocation_entry.get_type() {
        R_X86_64_32 | R_X86_64_32S => {
            let target_range = target_offset..(target_offset + size_of::<u32>());
            let target_ref = &mut target_slice[target_range];
            let source_value = source_addr.wrapping_add(relocation_entry.get_addend()) as u32;

            target_ref.copy_from_slice(&source_value.to_ne_bytes());
        }
        R_X86_64_PC32 | R_X86_64_PLT32 => {
            let target_range = target_offset..(target_offset + size_of::<u32>());
            let target_ref = &mut target_slice[target_range];
            let source_value = source_addr
                .wrapping_add(relocation_entry.get_addend())
                .wrapping_sub(target_ref.as_ptr() as usize as u64)
                as u32;

            target_ref.copy_from_slice(&source_value.to_ne_bytes());
        }
        R_X86_64_64 => {
            let target_range = target_offset..(target_offset + size_of::<u64>());
            let target_ref = &mut target_slice[target_range];
            let source_value = source_addr.wrapping_add(relocation_entry.get_addend());

            target_ref.copy_from_slice(&source_value.to_ne_bytes());
        }
        R_X86_64_PC64 => {
            let target_range = target_offset..(target_offset + size_of::<u64>());
            let target_ref = &mut target_slice[target_range];
            let source_val = source_addr
                .wrapping_add(relocation_entry.get_addend())
                .wrapping_sub(target_ref.as_ptr() as usize as u64);

            target_ref.copy_from_slice(&source_val.to_ne_bytes());
        }

        other => {
            error!("Unsupported relocation type: {other}");
            return Err("unsupported relocation type");
        }
    }

    Ok(())
}



// TODO: This needs to be thoroughly tested.
fn allocate_section_mappings(
    object_name: &str,
    elf_file: &ElfFile,
) -> Result<SectionMappings, &'static str> {
    let (executable_len, read_only_len, read_write_len): (usize, usize, usize) = {
        let mut executable_len = 0;
        let mut read_only_len = 0;
        let mut read_write_len = 0;

        for (section_index, section) in elf_file.section_iter().enumerate() {
            let section_flags = section.flags();

            // Skip non-allocated sections; they don't need to be loaded into memory.
            if section_flags & SHF_ALLOC == 0 {
                continue;
            }

            let name = section.get_name(elf_file);

            // Zero-sized sections may be aliased references to the next section in the ELF
            // file, but only if they have the same offset. Ignore the empty .text section
            // at the start.
            let section = if section.size() == 0 && name != Ok(".text") {
                let next_sec = elf_file
                    .get_section_header((section_index + 1) as u16)
                    .map_err(|_| "couldn't get next section for a zero-sized section")?;
                if next_sec.offset() == section.offset() {
                    next_sec
                } else {
                    section
                }
            } else {
                section
            };

            let size = section.size() as usize;
            let align = section.align() as usize;
            let offset = section.offset() as usize;
            let addend = size.next_multiple_of(align);

            let is_write = section_flags & SHF_WRITE == SHF_WRITE;
            let is_exec = section_flags & SHF_EXECINSTR == SHF_EXECINSTR;
            let is_tls = section_flags & SHF_TLS == SHF_TLS;

            // .text
            if is_exec {
                executable_len = executable_len.max(offset + addend);
            }
            // .tdata (.tbss sections are ignored)
            else if is_tls {
                if section.get_type() == Ok(SectionHeaderType::ProgBits) {
                    read_only_len += addend;
                }
            }
            // .bss and .data
            else if is_write {
                read_write_len += addend;
            }
            // .rodata, .eh_frame, and .gcc_except_table
            else {
                read_only_len += addend;
            }
        }

        (executable_len, read_only_len, read_write_len)
    };

    let flags =
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    Ok(SectionMappings {
        executable: (executable_len > 0)
            .then(|| KernelMapping::new(format!("{object_name}.x"), executable_len, flags)),
        read_only: (read_only_len > 0)
            .then(|| KernelMapping::new(format!("{object_name}.r"), read_only_len, flags)),
        read_write: (read_write_len > 0)
            .then(|| KernelMapping::new(format!("{object_name}.w"), read_write_len, flags)),
    })
}

struct SectionMappings {
    executable: Option<KernelMapping>,
    read_only: Option<KernelMapping>,
    read_write: Option<KernelMapping>,
}



pub fn crate_names_in_symbol(symbol_name: &str) -> Vec<&str> {
    let mut ranges = crate_name_ranges_in_symbol(symbol_name);
    ranges.dedup();

    ranges
        .into_iter()
        .filter_map(|range| symbol_name.get(range))
        .collect()
}

fn crate_name_ranges_in_symbol(symbol_name: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut start_bound = Some(0);
    while let Some(start) = start_bound {
        // The crate name will be right before the first occurrence of "::".
        let end = symbol_name
            .get(start..)
            .and_then(|s| s.find("::"))
            .map(|end_index| start + end_index);

        // If the substring (start..end) contains " as ", skip it and let the next
        // iteration of the loop handle it to avoid counting it twice.
        if let Some(end) = end {
            let substring = symbol_name.get(start..end);
            if substring.is_some_and(|s| !s.contains(" as ")) {
                // Find the beginning of the crate name, searching backwards from `end`. If
                // there was no non-name character, then the crate name started at the beginning
                // of `substring`.
                let start = substring
                    .and_then(|s| s.rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_')))
                    // Move forward to the actual start of the crate name.
                    .map(|start_index| start + start_index + 1)
                    .unwrap_or(start);

                if start != end {
                    ranges.push(start..end);
                }
            }
        }

        if let Some(end) = symbol_name
            .get(start..)
            .and_then(|s| s.find("["))
            .map(|end_index| start + end_index)
        {
            let substring = symbol_name.get(start..end);
            if substring.is_some() {
                // Find the beginning of the crate name, searching backwards from `end`. If
                // there was no non-name character, then the crate name started at the beginning
                // of `substring`.
                let start = substring
                    .and_then(|s| s.rfind(|ch: char| !(ch.is_alphanumeric() || ch == '_')))
                    // Move forward to the actual start of the crate name.
                    .map(|start_index| start + start_index + 1)
                    .unwrap_or(start);

                if start != end {
                    ranges.push(start..end);
                }
            }
        }

        // Advance to the next substring.
        start_bound = symbol_name
            .get(start..)
            .and_then(|s| s.find(" as "))
            .map(|start_index| start + start_index + " as ".len());
    }

    ranges
}
