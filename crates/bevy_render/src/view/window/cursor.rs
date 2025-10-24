use bevy_asset::{AssetId, Assets, Handle};
use bevy_ecs::{
    change_detection::DetectChanges as _,
    component::Component,
    entity::Entity,
    query::With,
    reflect::ReflectComponent,
    system::{Commands, Local, Query, Res},
    world::Ref,
};
use bevy_reflect::{std_traits::ReflectDefault, Reflect};
use bevy_utils::{tracing::warn, HashSet};
use bevy_window::{SystemCursorIcon, Window};
use bevy_winit::{
    convert_system_cursor_icon, CursorSource, CustomCursorCache, CustomCursorCacheKey,
    PendingCursor,
};

use crate::texture::Image;

#[derive(Component, Debug, Clone, Reflect, PartialEq, Eq)]
#[reflect(Component, Debug, Default)]
pub enum CursorIcon {
    Custom(CustomCursor),
    System(SystemCursorIcon),
}

impl Default for CursorIcon {
    fn default() -> Self {
        CursorIcon::System(Default::default())
    }
}

impl From<SystemCursorIcon> for CursorIcon {
    fn from(icon: SystemCursorIcon) -> Self {
        CursorIcon::System(icon)
    }
}

impl From<CustomCursor> for CursorIcon {
    fn from(cursor: CustomCursor) -> Self {
        CursorIcon::Custom(cursor)
    }
}

#[derive(Debug, Clone, Reflect, PartialEq, Eq, Hash)]
pub enum CustomCursor {
    Image {
        handle: Handle<Image>,
        hotspot: (u16, u16),
    },
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    Url { url: String, hotspot: (u16, u16) },
}

pub fn update_cursors(
    mut commands: Commands,
    mut windows: Query<(Entity, Ref<CursorIcon>), With<Window>>,
    cursor_cache: Res<CustomCursorCache>,
    images: Res<Assets<Image>>,
    mut queue: Local<HashSet<Entity>>,
) {
    for (entity, cursor) in windows.iter_mut() {
        if !(queue.remove(&entity) || cursor.is_changed()) {
            continue;
        }

        let cursor_source = match cursor.as_ref() {
            CursorIcon::Custom(CustomCursor::Image { handle, hotspot }) => {
                let cache_key = match handle.id() {
                    AssetId::Index { index, .. } => {
                        CustomCursorCacheKey::AssetIndex(index.to_bits())
                    }
                    AssetId::Uuid { uuid } => CustomCursorCacheKey::AssetUuid(uuid.as_u128()),
                };

                if cursor_cache.0.contains_key(&cache_key) {
                    CursorSource::CustomCached(cache_key)
                } else {
                    let Some(image) = images.get(handle) else {
                        warn!("Cursor image {handle:?} is not loaded yet and could not be used. Trying again next frame.");
                        queue.insert(entity);
                        continue;
                    };
                    let Some(rgba) = image_to_rgba_pixels(image) else {
                        warn!("Cursor image {handle:?} not accepted because it's not rgb8 or rgb32float format");
                        continue;
                    };

                    let width = image.texture_descriptor.size.width;
                    let height = image.texture_descriptor.size.height;
                    let source = match bevy_winit::WinitCustomCursor::from_rgba(
                        rgba,
                        width as u16,
                        height as u16,
                        hotspot.0,
                        hotspot.1,
                    ) {
                        Ok(source) => source,
                        Err(err) => {
                            warn!("Cursor image {handle:?} is invalid: {err}");
                            continue;
                        }
                    };

                    CursorSource::Custom((cache_key, source))
                }
            }
            #[cfg(all(target_family = "wasm", target_os = "unknown"))]
            CursorIcon::Custom(CustomCursor::Url { url, hotspot }) => {
                let cache_key = CustomCursorCacheKey::Url(url.clone());

                if cursor_cache.0.contains_key(&cache_key) {
                    CursorSource::CustomCached(cache_key)
                } else {
                    use bevy_winit::CustomCursorExtWebSys;
                    let source =
                        bevy_winit::WinitCustomCursor::from_url(url.clone(), hotspot.0, hotspot.1);
                    CursorSource::Custom((cache_key, source))
                }
            }
            CursorIcon::System(system_cursor_icon) => {
                CursorSource::System(convert_system_cursor_icon(*system_cursor_icon))
            }
        };

        commands
            .entity(entity)
            .insert(PendingCursor(Some(cursor_source)));
    }
}

fn image_to_rgba_pixels(image: &Image) -> Option<Vec<u8>> {
    match image.texture_descriptor.format {
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Rgba8Snorm
        | wgpu::TextureFormat::Rgba8Uint
        | wgpu::TextureFormat::Rgba8Sint => Some(image.data.clone()),
        wgpu::TextureFormat::Rgba32Float => Some(
            image
                .data
                .chunks(4)
                .map(|chunk| {
                    let chunk = chunk.try_into().unwrap();
                    let num = bytemuck::cast_ref::<[u8; 4], f32>(chunk);
                    (num * 255.0) as u8
                })
                .collect(),
        ),
        _ => None,
    }
}
