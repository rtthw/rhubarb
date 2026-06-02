//! # Virtual I/O GPU Device

use {
    crate::VirtqueueMessage,
    alloc::boxed::Box,
    core::{
        fmt,
        mem::MaybeUninit,
        sync::atomic::{AtomicU32, Ordering},
    },
};



pub struct Device {
    virtio_device: super::Device,
    control_queue: super::Virtqueue<64, { size_of::<Message>() }>,
    // cursor_queue: super::Virtqueue<16, { size_of::<Message>() }>,
}

impl Device {
    pub fn new(pci_device: pci::Device) -> Result<Self, &'static str> {
        let mut virtio_device = super::Device::new(pci_device)?;
        let control_queue = virtio_device.initialize(0, |dev| dev.initialize_queue(0));

        Ok(Self {
            virtio_device,
            control_queue,
            // cursor_queue,
        })
    }

    fn send_control(&mut self, message: Message) -> Message {
        unsafe {
            self.control_queue
                .push(&[
                    VirtqueueMessage::DeviceRead {
                        data: message,
                        len: None,
                    },
                    VirtqueueMessage::DeviceWrite,
                ])
                .unwrap();
            self.control_queue.notify_device();
        }

        loop {
            if let Some(responses) = unsafe { self.control_queue.pop::<2, Message>() } {
                break responses[1].unwrap();
            }
        }
    }

    fn send_control_without_response(&mut self, message: Message) -> Result<(), ()> {
        let resp = self.send_control(message);
        let resp: ControlHeader = unsafe { resp.control_header };
        // debug!("RESPONSE: {resp:?}");
        if resp.type_ == ControlType::VIRTIO_GPU_RESP_OK_NODATA as u32 {
            Ok(())
        } else {
            // log::debug!("Response type: {:#x}", resp.type_);
            Err(())
        }
    }

    pub fn display_info(&mut self) -> DisplayInfo {
        let res = self.send_control(Message {
            control_header: ControlHeader {
                type_: ControlType::VIRTIO_GPU_CMD_GET_DISPLAY_INFO as u32,
                ..Default::default()
            },
        });

        unsafe { res.display_info_response }
    }

    pub fn initialize_framebuffer(
        &mut self,
        framebuffer: &mut Framebuffer,
        virtual_to_physical_addr: &impl Fn(usize) -> usize,
    ) {
        framebuffer.pixels.fill(0x11);

        self.send_control_without_response(Message {
            resource_create_2d: ResourceCreate2d {
                header: ControlHeader {
                    type_: ControlType::VIRTIO_GPU_CMD_RESOURCE_CREATE_2D as u32,
                    ..Default::default()
                },
                resource_id: framebuffer.resource_id,
                format: PixelFormat::VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM as u32,
                width: framebuffer.width,
                height: framebuffer.height,
            },
        })
        .unwrap();

        let fb_vaddr = framebuffer.pixels.as_ptr() as usize;
        let fb_addr = virtual_to_physical_addr(fb_vaddr);

        // log::debug!(
        //     "FRAMEBUFFER_ADDR: {fb_vaddr:x} .. {:x} ({} bytes) | {fb_addr:x}",
        //     fb_vaddr + framebuffer.pixels.len(),
        //     framebuffer.pixels.len(),
        // );

        self.send_control_without_response(Message {
            resource_attach_backing: ResourceAttachBacking {
                header: ControlHeader {
                    type_: ControlType::VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING as u32,
                    ..Default::default()
                },
                resource_id: framebuffer.resource_id,
                entry_count: 1,
                entries: {
                    let mut entries = [MemEntry::default(); MAX_MEM_PAGES];
                    entries[0] = MemEntry {
                        addr: fb_addr as u64,
                        length: framebuffer.pixels.len() as u32,
                        _padding: 0,
                    };
                    entries
                },
            },
        })
        .unwrap();

        self.send_control_without_response(Message {
            set_scanout: SetScanout {
                header: ControlHeader {
                    type_: ControlType::VIRTIO_GPU_CMD_SET_SCANOUT as u32,
                    ..Default::default()
                },
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: framebuffer.width,
                    height: framebuffer.height,
                },
                scanout_id: 0,
                resource_id: framebuffer.resource_id,
            },
        })
        .unwrap();
    }

    pub fn flush(&mut self, framebuffer: &mut Framebuffer) {
        self.send_control_without_response(Message {
            transfer_to_host_2d: TransferToHost2d {
                header: ControlHeader {
                    type_: ControlType::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D as u32,
                    ..Default::default()
                },
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: framebuffer.width,
                    height: framebuffer.height,
                },
                offset: 0,
                resource_id: framebuffer.resource_id,
                _padding: 0,
            },
        })
        .unwrap();

        self.send_control_without_response(Message {
            resource_flush: ResourceFlush {
                header: ControlHeader {
                    type_: ControlType::VIRTIO_GPU_CMD_RESOURCE_FLUSH as u32,
                    ..Default::default()
                },
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: framebuffer.width,
                    height: framebuffer.height,
                },
                resource_id: framebuffer.resource_id,
                _padding: 0,
            },
        })
        .unwrap();
    }
}

static NEXT_RESOURCE_ID: AtomicU32 = AtomicU32::new(1);

pub struct Framebuffer {
    resource_id: u32,
    pixels: Box<[u8]>,
    width: u32,
    height: u32,
}

impl Framebuffer {
    pub fn new(mode: &DisplayMode) -> Self {
        let width = mode.rect.width;
        let height = mode.rect.height;

        Self {
            resource_id: NEXT_RESOURCE_ID.fetch_add(1, Ordering::SeqCst),
            pixels: alloc::vec![0; (width * height * 4) as usize].into_boxed_slice(),
            width,
            height,
        }
    }
}



#[derive(Clone, Copy)]
#[repr(C)]
union Message {
    display_info_response: DisplayInfo,
    resource_create_2d: ResourceCreate2d,
    resource_attach_backing: ResourceAttachBacking,
    set_scanout: SetScanout,
    transfer_to_host_2d: TransferToHost2d,
    resource_flush: ResourceFlush,
    control_header: ControlHeader,
}

impl Default for Message {
    fn default() -> Self {
        let x = MaybeUninit::<Self>::zeroed();
        unsafe { x.assume_init() }
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Message({:?})", unsafe { self.control_header })
    }
}

#[allow(non_camel_case_types, unused)]
#[repr(u32)]
enum ControlType {
    VIRTIO_GPU_CMD_GET_DISPLAY_INFO = 0x0100,
    VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
    VIRTIO_GPU_CMD_RESOURCE_UNREF,
    VIRTIO_GPU_CMD_SET_SCANOUT,
    VIRTIO_GPU_CMD_RESOURCE_FLUSH,
    VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
    VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING,
    VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING,
    VIRTIO_GPU_CMD_GET_CAPSET_INFO,
    VIRTIO_GPU_CMD_GET_CAPSET,
    VIRTIO_GPU_CMD_GET_EDID,
    VIRTIO_GPU_CMD_RESOURCE_ASSIGN_UUID,
    VIRTIO_GPU_CMD_RESOURCE_CREATE_BLOB,
    VIRTIO_GPU_CMD_SET_SCANOUT_BLOB,

    VIRTIO_GPU_CMD_CTX_CREATE = 0x0200,
    VIRTIO_GPU_CMD_CTX_DESTROY,
    VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE,
    VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE,
    VIRTIO_GPU_CMD_RESOURCE_CREATE_3D,
    VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D,
    VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D,
    VIRTIO_GPU_CMD_SUBMIT_3D,
    VIRTIO_GPU_CMD_RESOURCE_MAP_BLOB,
    VIRTIO_GPU_CMD_RESOURCE_UNMAP_BLOB,

    VIRTIO_GPU_CMD_UPDATE_CURSOR = 0x0300,
    VIRTIO_GPU_CMD_MOVE_CURSOR,

    VIRTIO_GPU_RESP_OK_NODATA = 0x1100,
    VIRTIO_GPU_RESP_OK_DISPLAY_INFO,
    VIRTIO_GPU_RESP_OK_CAPSET_INFO,
    VIRTIO_GPU_RESP_OK_CAPSET,
    VIRTIO_GPU_RESP_OK_EDID,
    VIRTIO_GPU_RESP_OK_RESOURCE_UUID,
    VIRTIO_GPU_RESP_OK_MAP_INFO,

    VIRTIO_GPU_RESP_ERR_UNSPEC = 0x1200,
    VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY,
    VIRTIO_GPU_RESP_ERR_INVALID_SCANOUT_ID,
    VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID,
    VIRTIO_GPU_RESP_ERR_INVALID_CONTEXT_ID,
    VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
}

#[allow(non_camel_case_types, unused)]
#[repr(u32)]
enum PixelFormat {
    VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM = 1,
    VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM = 2,
    VIRTIO_GPU_FORMAT_A8R8G8B8_UNORM = 3,
    VIRTIO_GPU_FORMAT_X8R8G8B8_UNORM = 4,
    VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM = 67,
    VIRTIO_GPU_FORMAT_X8B8G8R8_UNORM = 68,
    VIRTIO_GPU_FORMAT_A8B8G8R8_UNORM = 121,
    VIRTIO_GPU_FORMAT_R8G8B8X8_UNORM = 134,
}

const VIRTIO_GPU_FLAG_FENCE: u32 = 1 << 0;
const VIRTIO_GPU_MAX_SCANOUTS: usize = 16;

#[derive(Clone, Copy)]
#[repr(C)]
struct ControlHeader {
    type_: u32,
    flags: u32,
    fence_id: u64,
    /// Rendering context (used in 3D mode only).
    context_id: u32,
    ring_index: u8,
    _padding: [u8; 3],
}

impl fmt::Debug for ControlHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ControlHeader")
            .field("type", &self.type_)
            .field("flags", &self.flags)
            .field("fence_id", &self.fence_id)
            .field("context_id", &self.context_id)
            .field("ring_index", &self.ring_index)
            .finish()
    }
}

impl Default for ControlHeader {
    fn default() -> Self {
        Self {
            type_: 0,
            flags: VIRTIO_GPU_FLAG_FENCE,
            fence_id: 0,
            context_id: 0,
            ring_index: 0,
            _padding: [0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct DisplayInfo {
    header: ControlHeader,
    pub modes: [DisplayMode; VIRTIO_GPU_MAX_SCANOUTS],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct DisplayMode {
    pub rect: Rect,
    pub enabled: u32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct ResourceCreate2d {
    header: ControlHeader,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SetScanout {
    header: ControlHeader,
    rect: Rect,
    scanout_id: u32,
    resource_id: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct TransferToHost2d {
    header: ControlHeader,
    rect: Rect,
    offset: u64,
    resource_id: u32,
    _padding: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ResourceFlush {
    header: ControlHeader,
    rect: Rect,
    resource_id: u32,
    _padding: u32,
}

const MAX_MEM_PAGES: usize = 1;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ResourceAttachBacking {
    header: ControlHeader,
    resource_id: u32,
    entry_count: u32,
    entries: [MemEntry; MAX_MEM_PAGES],
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MemEntry {
    addr: u64,
    length: u32,
    _padding: u32,
}

impl Default for MemEntry {
    fn default() -> Self {
        Self {
            addr: 0,
            length: 0,
            _padding: 0,
        }
    }
}
