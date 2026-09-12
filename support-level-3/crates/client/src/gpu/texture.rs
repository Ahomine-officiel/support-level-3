//! Chargement de textures PNG -> vue wgpu + samplers.

use image::ImageFormat;
use std::path::Path;
use wgpu::*;

pub struct GpuTexture {
    pub view: TextureView,
}

pub fn load_png(device: &Device, queue: &Queue, path: &Path) -> GpuTexture {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("texture manquante {}: {}", path.display(), e));
    let img = image::load_from_memory_with_format(&data, ImageFormat::Png)
        .expect("PNG invalide")
        .to_rgba8();
    let (w, h) = img.dimensions();
    let tex = device.create_texture(&TextureDescriptor {
        label: Some(path.to_str().unwrap_or("tex")),
        size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &img.into_raw(),
        ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: None,
        },
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    GpuTexture { view: tex.create_view(&TextureViewDescriptor::default()) }
}

pub fn white_texture(device: &Device, queue: &Queue) -> GpuTexture {
    let tex = device.create_texture(&TextureDescriptor {
        label: Some("white"),
        size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[255, 255, 255, 255],
        ImageDataLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None },
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    GpuTexture { view: tex.create_view(&TextureViewDescriptor::default()) }
}

pub fn linear_sampler(device: &Device, repeat: bool) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("linear"),
        address_mode_u: if repeat { AddressMode::Repeat } else { AddressMode::ClampToEdge },
        address_mode_v: if repeat { AddressMode::Repeat } else { AddressMode::ClampToEdge },
        address_mode_w: if repeat { AddressMode::Repeat } else { AddressMode::ClampToEdge },
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Nearest,
        ..Default::default()
    })
}
