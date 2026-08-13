use crate::{Bounds, DevicePixels, Size};
use anyhow::{Result, bail};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_GPU_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Identifies one lifetime of GPUI's graphics device.
///
/// Device loss creates a new context with a new epoch. GPU resources tagged
/// with an older epoch must not be submitted to the replacement device.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GpuDeviceEpoch(u64);

impl GpuDeviceEpoch {
    /// Returns the monotonically increasing epoch value.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A snapshot of the graphics device shared by GPUI and application renderers.
///
/// Cloning this value keeps the device alive. Callers should compare
/// [`Self::epoch`] before reusing resources across device-loss recovery.
#[derive(Clone)]
pub struct GpuContext {
    epoch: GpuDeviceEpoch,
    instance: Arc<wgpu::Instance>,
    adapter: Arc<wgpu::Adapter>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl GpuContext {
    /// Constructs a graphics-context snapshot for a platform renderer.
    ///
    /// Application code obtains snapshots from [`Window::gpu_context`] rather
    /// than constructing them directly.
    #[doc(hidden)]
    pub fn new(
        epoch: u64,
        instance: Arc<wgpu::Instance>,
        adapter: Arc<wgpu::Adapter>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Self {
        Self {
            epoch: GpuDeviceEpoch(epoch),
            instance,
            adapter,
            device,
            queue,
        }
    }

    /// Returns the device lifetime represented by this snapshot.
    pub fn epoch(&self) -> GpuDeviceEpoch {
        self.epoch
    }

    /// Returns GPUI's shared wgpu instance.
    pub fn instance(&self) -> &Arc<wgpu::Instance> {
        &self.instance
    }

    /// Returns GPUI's shared wgpu adapter.
    pub fn adapter(&self) -> &Arc<wgpu::Adapter> {
        &self.adapter
    }

    /// Returns GPUI's shared wgpu device.
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    /// Returns GPUI's shared wgpu queue.
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    /// Returns the adapter information associated with this device.
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }

    /// Returns the features enabled on this device.
    pub fn features(&self) -> wgpu::Features {
        self.device.features()
    }

    /// Returns the limits enabled on this device.
    pub fn limits(&self) -> wgpu::Limits {
        self.device.limits()
    }

    /// Creates an application texture tagged with this context's device epoch.
    pub fn create_texture(&self, descriptor: &wgpu::TextureDescriptor<'_>) -> GpuTexture {
        GpuTexture {
            epoch: self.epoch,
            texture: Arc::new(self.device.create_texture(descriptor)),
        }
    }
}

impl std::fmt::Debug for GpuContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GpuContext")
            .field("epoch", &self.epoch)
            .field("adapter", &self.adapter.get_info())
            .field("features", &self.device.features())
            .field("limits", &self.device.limits())
            .finish_non_exhaustive()
    }
}

/// Identifies an application-owned texture retained by a [`GpuImage`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GpuImageId(u64);

/// Describes how alpha is stored in a [`GpuImage`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuImageAlphaMode {
    /// Every texel is fully opaque.
    Opaque,
    /// RGB channels are independent of alpha.
    Straight,
    /// RGB channels have already been multiplied by alpha.
    Premultiplied,
}

/// Describes the color values stored in a [`GpuImage`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuImageColorEncoding {
    /// Scene-linear values in GPUI's default linear-sRGB interchange space.
    SceneLinear,
    /// Display-linear values in GPUI's default linear-sRGB display space.
    DisplayLinear,
    /// Values already encoded for the target display.
    DisplayEncoded,
}

/// Sampling used when a [`GpuImage`] is painted at a different size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuImageSampling {
    /// Select the nearest source texel.
    Nearest,
    /// Bilinearly interpolate neighboring source texels.
    Linear,
}

/// An application texture created from GPUI's shared graphics device.
#[derive(Debug)]
pub struct GpuTexture {
    epoch: GpuDeviceEpoch,
    texture: Arc<wgpu::Texture>,
}

impl GpuTexture {
    /// Returns the device epoch on which this texture was created.
    pub fn epoch(&self) -> GpuDeviceEpoch {
        self.epoch
    }

    /// Returns the underlying wgpu texture for application rendering.
    pub fn texture(&self) -> &Arc<wgpu::Texture> {
        &self.texture
    }
}

/// An opaque, device-epoch-tagged image that GPUI can sample directly.
///
/// The texture is not copied into GPUI's CPU image cache or sprite atlas. The
/// creating application may retain another [`Arc`] to continue rendering into
/// the texture before submitting a frame that references this image.
pub struct GpuImage {
    id: GpuImageId,
    epoch: GpuDeviceEpoch,
    texture: Arc<wgpu::Texture>,
    view: wgpu::TextureView,
    size: Size<DevicePixels>,
    alpha_mode: GpuImageAlphaMode,
    color_encoding: GpuImageColorEncoding,
}

impl GpuImage {
    /// Retains a texture created from `context` for direct scene composition.
    pub fn new(
        context: &GpuContext,
        texture: GpuTexture,
        alpha_mode: GpuImageAlphaMode,
        color_encoding: GpuImageColorEncoding,
    ) -> Result<Arc<Self>> {
        if texture.epoch != context.epoch {
            bail!(
                "GPU texture belongs to device epoch {}, but the context epoch is {}",
                texture.epoch.get(),
                context.epoch.get(),
            );
        }
        let texture = texture.texture;
        let extent = texture.size();
        if texture.dimension() != wgpu::TextureDimension::D2 || extent.depth_or_array_layers != 1 {
            bail!("GPU images must use a single-layer 2D texture");
        }
        if texture.sample_count() != 1 {
            bail!("GPU images must not be multisampled");
        }
        if !texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            bail!("GPU image texture is missing TEXTURE_BINDING usage");
        }
        if texture.format().is_srgb() {
            bail!(
                "GPU images require a non-sRGB texture format so color encoding remains explicit"
            );
        }
        let format_features = context
            .adapter()
            .get_texture_format_features(texture.format());
        if !format_features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        {
            bail!("GPU image texture format must support filtering");
        }
        let width = i32::try_from(extent.width)?;
        let height = i32::try_from(extent.height)?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Arc::new(Self {
            id: GpuImageId(NEXT_GPU_IMAGE_ID.fetch_add(1, Ordering::Relaxed)),
            epoch: context.epoch(),
            texture,
            view,
            size: Size::new(DevicePixels(width), DevicePixels(height)),
            alpha_mode,
            color_encoding,
        }))
    }

    /// Returns this image's process-local identity.
    pub fn id(&self) -> GpuImageId {
        self.id
    }

    /// Returns the device epoch on which this image was created.
    pub fn epoch(&self) -> GpuDeviceEpoch {
        self.epoch
    }

    /// Returns the texture's dimensions.
    pub fn size(&self) -> Size<DevicePixels> {
        self.size
    }

    /// Returns the full source rectangle of this image.
    pub fn bounds(&self) -> Bounds<DevicePixels> {
        Bounds::new(Default::default(), self.size)
    }

    /// Returns the image's alpha representation.
    pub fn alpha_mode(&self) -> GpuImageAlphaMode {
        self.alpha_mode
    }

    /// Returns the image's color encoding.
    pub fn color_encoding(&self) -> GpuImageColorEncoding {
        self.color_encoding
    }

    #[doc(hidden)]
    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }

    #[doc(hidden)]
    pub fn texture(&self) -> &Arc<wgpu::Texture> {
        &self.texture
    }
}

impl std::fmt::Debug for GpuImage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GpuImage")
            .field("id", &self.id)
            .field("epoch", &self.epoch)
            .field("size", &self.size)
            .field("format", &self.texture.format())
            .field("alpha_mode", &self.alpha_mode)
            .field("color_encoding", &self.color_encoding)
            .finish_non_exhaustive()
    }
}
