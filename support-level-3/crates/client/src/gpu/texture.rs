//! Chargement de textures PNG -> vue wgpu + samplers.
//! v2 : chaîne de mipmaps complète (filtre box en espace linéaire, upload paddé)
//! + sampler anisotrope — moins d'aliasing, meilleure bande passante sur iGPU.

use image::ImageFormat;
use std::path::Path;
use wgpu::*;

pub struct GpuTexture {
    pub view: TextureView,
}

/// Réaligne chaque rangée sur un multiple de 256 octets (validité WebGPU).
fn padded_rows(data: &[u8], w: u32, h: u32) -> Vec<u8> {
    let raw = (w * 4) as usize;
    let bpr = (raw + 255) / 256 * 256;
    if bpr == raw {
        return data.to_vec();
    }
    let mut out = vec![0u8; bpr * h as usize];
    for row in 0..h as usize {
        out[row * bpr..row * bpr + raw].copy_from_slice(&data[row * raw..(row + 1) * raw]);
    }
    out
}

fn bytes_per_row(w: u32) -> u32 {
    (w * 4 + 255) / 256 * 256
}

/// Réduction /2 par filre box, en espace linéaire (évite le bouillonnement gamma).
fn downsample(src: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let to_lin = |c: u8| -> f32 { (c as f32 / 255.0).powf(2.2) };
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    let (wu, nwu) = (w as usize, nw as usize);
    for y in 0..nh {
        let sy = (y * 2).min(h - 1) as usize;
        let sy2 = (y * 2 + 1).min(h - 1) as usize;
        for x in 0..nw {
            let sx = (x * 2).min(w - 1) as usize;
            let sx2 = (x * 2 + 1).min(w - 1) as usize;
            for ch in 0..4 {
                let a = to_lin(src[(sy * wu + sx) * 4 + ch]);
                let b = to_lin(src[(sy * wu + sx2) * 4 + ch]);
                let c = to_lin(src[(sy2 * wu + sx) * 4 + ch]);
                let d = to_lin(src[(sy2 * wu + sx2) * 4 + ch]);
                let avg = (a + b + c + d) * 0.25;
                out[(y as usize * nwu + x as usize) * 4 + ch] =
                    ((avg.powf(1.0 / 2.2)) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    (out, nw, nh)
}

pub fn load_png(device: &Device, queue: &Queue, path: &Path) -> GpuTexture {
    load_png_opts(device, queue, path, true)
}

/// `with_mips = false` pour les atlas UI (texte net à toute taille).
pub fn load_png_opts(device: &Device, queue: &Queue, path: &Path, with_mips: bool) -> GpuTexture {
    let data = std::fs::read(path)
        .unwrap_or_else(|e| panic!("texture manquante {}: {}", path.display(), e));
    let img = image::load_from_memory_with_format(&data, ImageFormat::Png)
        .expect("PNG invalide")
        .to_rgba8();
    let (w, h) = img.dimensions();
    let mips = if with_mips { w.max(h).max(1).ilog2() + 1 } else { 1 };
    let tex = device.create_texture(&TextureDescriptor {
        label: Some(path.to_str().unwrap_or("tex")),
        size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level_data = img.into_raw();
    let (mut lw, mut lh) = (w, h);
    for level in 0..mips {
        queue.write_texture(
            ImageCopyTexture {
                texture: &tex,
                mip_level: level,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &padded_rows(&level_data, lw, lh),
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row(lw)),
                rows_per_image: None,
            },
            Extent3d { width: lw, height: lh, depth_or_array_layers: 1 },
        );
        if level + 1 < mips {
            let (down, nw, nh) = downsample(&level_data, lw, lh);
            level_data = down;
            lw = nw;
            lh = nh;
        }
    }
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
        &padded_rows(&[255, 255, 255, 255], 1, 1),
        ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row(1)),
            rows_per_image: None,
        },
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    GpuTexture { view: tex.create_view(&TextureViewDescriptor::default()) }
}

/// Texture 1x1 rgba16float (demi-flots bruts) — valeurs neutres du tampon RT
/// quand le ray tracing est désactivé : (1, 1, 0, 1) = AO 1, ombres 1, GI 0.
pub fn neutral_rt_texture(device: &Device, queue: &Queue, label: &str, rgba16: [u16; 4]) -> GpuTexture {
    let tex = device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: [u8; 8] = bytemuck::cast_slice(&rgba16).try_into().unwrap();
    queue.write_texture(
        ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &bytes,
        ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: None,
        },
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    GpuTexture { view: tex.create_view(&TextureViewDescriptor::default()) }
}

pub fn linear_sampler(device: &Device, repeat: bool) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("linear-mips"),
        address_mode_u: if repeat { AddressMode::Repeat } else { AddressMode::ClampToEdge },
        address_mode_v: if repeat { AddressMode::Repeat } else { AddressMode::ClampToEdge },
        address_mode_w: if repeat { AddressMode::Repeat } else { AddressMode::ClampToEdge },
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        anisotropy_clamp: 8,
        ..Default::default()
    })
}
