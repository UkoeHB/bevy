use crate::view::ExtractedRenderGroups;

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::{Component, Entity, ReflectComponent};
use bevy_reflect::Reflect;
use bevy_reflect::prelude::ReflectDefault;

use smallvec::SmallVec;

/// The default [`RenderLayer`].
pub static DEFAULT_RENDER_LAYER: RenderLayer = RenderLayer(0);

/// Wraps a specific render layer that can be stored in [`RenderXXLayersXX`].
///
/// Stores an index into the [`RenderXXLayersXX`] internal bitmask.
//todo: Upper limit policy for render layer indices.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Deref, DerefMut)]
pub struct RenderLayer(pub usize);

impl RenderLayer {
    /// Returns `true` if equal to [`DEFAULT_RENDER_LAYER`].
    pub fn is_default(&self) -> bool {
        *self == DEFAULT_RENDER_LAYER
    }
}

impl From<usize> for RenderLayer {
    fn from(layer: usize) -> Self {
        Self(layer)
    }
}

impl Default for RenderLayer {
    fn default() -> Self {
        DEFAULT_RENDER_LAYER
    }
}

/// Records a growable bitmask of flags for controlling which entities
/// are visible to which cameras.
///
/// Individual render layers can be defined with [`RenderLayer`], which is an index
/// into the internal `RenderXXLayersXX` bitmask.
///
/// `RenderXXLayersXX::default()` starts with [`DEFAULT_RENDER_LAYER`], which is the global default
/// layer.
///
/// ### Performance
///
/// `RenderXXLayersXX` occupies 24 bytes on the stack.
///
/// `RenderXXLayersXX` can store up to `RenderLayer(63)` without allocating. Allocations occur in 8-byte
/// increments, so the second allocation will occur after `RenderLayer(127)`, and so on.
#[derive(Debug, Clone, PartialEq, Reflect)]
#[reflect(Default, PartialEq)]
pub struct RenderXXLayersXX {
    layers: SmallVec<[u64; 1]>,
}

impl RenderXXLayersXX {
    /// Makes a new `RenderXXLayersXX` with no layers.
    pub fn empty() -> Self {
        Self {
            layers: SmallVec::default(),
        }
    }

    /// Makes a new `RenderXXLayersXX` from a slice.
    pub fn from_layers<T: Into<RenderLayer> + Copy>(layers: &[T]) -> Self {
        layers.iter().map(|l| (*l).into()).collect()
    }

    /// Adds a [`RenderLayer`].
    pub fn add(&mut self, layer: impl Into<RenderLayer>) -> &mut Self {
        let (buffer_index, bit) = Self::layer_info(*(layer.into()));
        self.extend_buffer(buffer_index + 1);
        self.layers[buffer_index] |= bit;
        self
    }

    /// Removes a [`RenderLayer`].
    ///
    /// Does not shrink the internal buffer even if doing so is possible after
    /// removing the layer. We assume if you added a large layer then it is
    /// possible you may re-add another large layer.
    pub fn remove(&mut self, layer: impl Into<RenderLayer>) -> &mut Self {
        let (buffer_index, bit) = Self::layer_info(*(layer.into()));
        if buffer_index >= self.layers.len() {
            return self;
        }
        self.layers[buffer_index] &= !bit;
        self
    }

    /// Clears all stored render layers without deallocating.
    pub fn clear(&mut self) {
        self.layers.clear();
    }

    /// Copies `other` into `Self`.
    ///
    /// This is more efficient than cloning `other` if you want to reuse a `RenderXXLayersXX`
    /// that is potentially allocated.
    pub fn set_from(&mut self, other: &Self) {
        self.layers.clear();
        self.layers.reserve_exact(other.layers.len());
        self.layers.extend_from_slice(other.layers.as_slice());
    }

    /// Merges `other` into `Self`.
    ///
    /// After merging, `Self` will include all set bits from `other` and `Self`.
    ///
    /// Will allocate if necessary to include all set bits of `other`.
    pub fn merge(&mut self, other: &Self) {
        self.extend_buffer(other.layers.len());

        for (self_layer, other_layer) in self.layers.iter_mut().zip(other.layers.iter()) {
            *self_layer |= *other_layer;
        }
    }

    /// Gets the number of stored layers.
    ///
    /// Equivalent to `self.iter().count()`.
    pub fn num_layers(&self) -> usize {
        self.iter().count()
    }

    /// Iterates the internal render layers.
    pub fn iter(&self) -> impl Iterator<Item = RenderLayer> + '_ {
        self.layers.iter().copied().map(Self::iter_layers).flatten()
    }

    /// Returns `true` if the specified render layer is included in this `RenderXXLayersXX`.
    pub fn contains(&self, layer: impl Into<RenderLayer>) -> bool {
        let (buffer_index, bit) = Self::layer_info(*(layer.into()));
        if buffer_index >= self.layers.len() {
            return false;
        }
        (self.layers[buffer_index] & bit) != 0
    }

    /// Returns `true` if `Self` and `other` contain any matching layers.
    pub fn intersects(&self, other: &Self) -> bool {
        for (self_layer, other_layer) in self.layers.iter().zip(other.layers.iter()) {
            if (*self_layer & *other_layer) != 0 {
                return true;
            }
        }

        false
    }

    /// Gets the bitmask representation of the contained layers
    /// as a slice of bitmasks.
    pub fn bits(&self) -> &[u64] {
        self.layers.as_slice()
    }

    fn layer_info(layer: usize) -> (usize, u64) {
        let buffer_index = layer / 64;
        let bit_index = layer % 64;
        let bit = 1u64 << bit_index;

        (buffer_index, bit)
    }

    fn extend_buffer(&mut self, other_len: usize) {
        let new_size = std::cmp::max(self.layers.len(), other_len);
        self.layers.reserve_exact(new_size - self.layers.len());
        self.layers.resize(new_size, 0u64);
    }

    fn iter_layers(mut buffer: u64) -> impl Iterator<Item = RenderLayer> + 'static {
        let mut layer: usize = 0;
        std::iter::from_fn(move || {
            if buffer == 0 {
                return None;
            }
            let next = buffer.trailing_zeros() + 1;
            buffer >>= next;
            layer += next as usize;
            Some(RenderLayer(layer - 1))
        })
    }
}

impl<T: Into<RenderLayer>> From<T> for RenderXXLayersXX {
    fn from(layer: T) -> Self {
        let mut layers = Self {
            layers: SmallVec::default(),
        };
        layers.add(layer);
        layers
    }
}

impl<R: Into<RenderLayer>> FromIterator<R> for RenderXXLayersXX {
    fn from_iter<T: IntoIterator<Item = R>>(i: T) -> Self {
        i.into_iter().fold(Self::empty(), |mut mask, g| {
            mask.add(g);
            mask
        })
    }
}

impl Default for RenderXXLayersXX {
    fn default() -> Self {
        Self::from(DEFAULT_RENDER_LAYER)
    }
}

/// Component on an entity that controls which cameras can see it.
///
/// There are two kinds of render groups:
/// - [`RenderXXLayersXX`]: These are grouping categories that many cameras can view (see [`CameraView`]).
/// - *Camera entity*: This is a specific camera that the entity is affiliated with. This is especially
///   useful for UI in combination with [`PropagateRenderGroups`].
///
/// An entity can be a member of multiple [`RenderXXLayersXX`] in addition to having a camera affiliation.
///
/// ### Default behavior
///
/// A default-constructed `RenderGroups` will include [`DEFAULT_RENDER_LAYER`].
/// If you don't want that, then use [`Self::empty`], [`Self::new_with_camera`], or
/// [`Self::from::<RenderLayer>`].
///
/// ### Entity default behavior
///
/// All entities without a [`RenderGroups`] component are in [`DEFAULT_RENDER_LAYER`] by
/// default (layer 0). If you add a [`RenderGroups`] component to an entity, it may no longer
/// be in the default layer if the [`RenderGroups`] component doesn't include it.
///
/// For example, if you do `entity.insert(RenderGroups::from(RenderLayer(1)))`, then `entity`
/// will only be in layer 1. You can instead do:
/**
```no-run
// Option 1: default
let mut groups = RenderGroups::default();
groups.add(RenderLayer(1);
entity.insert(groups);

// Option 2: explicit
let mut groups = RenderGroups::from(0);
groups.add(RenderLayer(1);
entity.insert(groups);
```
///
/// Similarly, if an entity without [`RenderGroups`] inherits from an entity with [`PropagateRenderGroups`] that
/// doesn't propagate layer 0, then the entity's computed [`InheritedRenderGroups`] won't have layer 0 and the
/// entity won't be visible to layer 0.
*/
#[derive(Component, Debug, Clone, Reflect, PartialEq)]
#[reflect(Component, Default, PartialEq)]
pub struct RenderGroups {
    layers: RenderXXLayersXX,
    camera: Option<Entity>,
}

impl RenderGroups {
    /// Makes a new `RenderGroups` with no groups.
    pub fn empty() -> Self {
        Self {
            layers: RenderXXLayersXX::empty(),
            camera: None,
        }
    }

    /// Makes a new `RenderGroups` with just a camera.
    pub fn new_with_camera(camera: Entity) -> Self {
        Self {
            layers: RenderXXLayersXX::empty(),
            camera: Some(camera),
        }
    }

    /// Makes a new `RenderGroups` with a camera and the [`DEFAULT_RENDER_LAYER`].
    pub fn default_with_camera(camera: Entity) -> Self {
        Self {
            layers: RenderXXLayersXX::default(),
            camera: Some(camera),
        }
    }

    /// Adds a [`RenderLayer`].
    ///
    /// See [`RenderXXLayersXX::add`].
    pub fn add(&mut self, layer: impl Into<RenderLayer>) -> &mut Self {
        self.layers.add(layer);
        self
    }

    /// Removes a [`RenderLayer`].
    ///
    /// See [`RenderXXLayersXX::remove`].
    pub fn remove(&mut self, layer: impl Into<RenderLayer>) -> &mut Self {
        self.layers.remove(layer);
        self
    }

    /// Clears all stored render layers without deallocating, and unsets the camera affiliation.
    pub fn clear(&mut self) {
        self.layers.clear();
        self.camera = None;
    }

    /// Merges `other` into `Self`.
    ///
    /// After merging, `Self` will include all [`RenderXXLayersXX`] from `other` and `Self`.
    /// If both `Self` and `other` have a camera affiliation, then the `Self` camera
    /// will be in the merged result. Otherwise the `other` camera will be in the result.
    ///
    /// Will allocate if necessary to include all [`RenderXXLayersXX`] of `other`.
    pub fn merge(&mut self, other: &Self) {
        self.layers.merge(&other.layers);
        self.camera = self.camera.or(other.camera);
    }

    /// Copies `other` into `Self.
    ///
    /// This is more efficient than cloning `other` if you want to reuse a `RenderGroups`
    /// that is potentially allocated.
    pub fn set_from(&mut self, other: &Self) {
        self.layers.set_from(&other.layers);
        self.camera = other.camera;
    }

    /// Sets the camera affiliation.
    ///
    /// Returns the previous camera.
    pub fn set_camera(&mut self, camera: Entity) -> Option<Entity> {
        self.camera.replace(camera)
    }

    /// Removes the current camera affiliation.
    ///
    /// Returns the removed camera.
    pub fn remove_camera(&mut self) -> Option<Entity> {
        self.camera.take()
    }

    /// Returns an iterator over [`RenderLayer`].
    pub fn iter_layers(&self) -> impl Iterator<Item = RenderLayer> + '_ {
        self.layers.iter()
    }

    /// Returns `true` if the specified render layer is included in this
    /// `RenderGroups`.
    pub fn contains_layer(&self, layer: impl Into<RenderLayer>) -> bool {
        self.layers.contains(layer)
    }

    /// Returns `true` if `Self` intersects with `other`.
    ///
    /// Checks both camera affiliation and [`RenderXXLayersXX`] intersection.
    pub fn intersects(&self, other: &Self) -> bool {
        if let (Some(a), Some(b)) = (self.camera, other.camera) {
            if a == b {
                return true;
            }
        }
        self.layers.intersects(&other.layers)
    }

    /// Returns `true` if `Self` intersects with an [`ExtractedRenderGroups`].
    ///
    /// If `extracted` is `None`, then intersections is tested using [`RenderGroups::default`].
    pub fn intersects_extracted(&self, extracted: Option<&ExtractedRenderGroups>) -> bool {
        let default_render_groups = RenderGroups::default();
        let render_groups = extracted.map(|i| &**i).unwrap_or(&default_render_groups);
        self.intersects(render_groups)
    }

    /// Gets the camera affiliation.
    pub fn camera(&self) -> Option<Entity> {
        self.camera
    }
}

impl From<RenderLayer> for RenderGroups {
    /// Makes a new `RenderGroups` from a specific [`RenderLayer`].
    fn from(layer: RenderLayer) -> Self {
        Self {
            layers: RenderXXLayersXX::from(layer),
            camera: None,
        }
    }
}

impl From<RenderXXLayersXX> for RenderGroups {
    /// Makes a new `RenderGroups` from a [`RenderXXLayersXX`].
    fn from(layers: RenderXXLayersXX) -> Self {
        Self {
            layers,
            camera: None,
        }
    }
}

impl Default for RenderGroups {
    /// Equivalent to `Self::from(DEFAULT_RENDER_LAYER)`.
    fn default() -> Self {
        Self::from(DEFAULT_RENDER_LAYER)
    }
}

/// Component on camera entities that controls which [`RenderXXLayersXX`] are visible to
/// the camera.
///
/// A camera will see any entity that satisfies either of these conditions:
/// - The entity is in a [`RenderLayer`] visible to the camera.
/// - The entity has a [`RenderGroups`] component with camera affiliation equal to the camera.
///
/// Cameras use entities' [`InheritedRenderGroups`] to determine visibility, with a fall-back to the
/// entity's [`RenderGroups`]. If an entity does not have [`InheritedRenderGroups`]
/// or [`RenderGroups`] components, then the camera will only see it if the camera can
/// view the [`DEFAULT_RENDER_LAYER`] layer.
///
/// A camera without the `CameraView` component will see the [`DEFAULT_RENDER_LAYER`]
/// layer, in addition to relevant [`RenderGroups`] camera affiliations.
///
/// A default `CameraView` will include the [`DEFAULT_RENDER_LAYER`].
#[derive(Component, Debug, Clone)]
pub struct CameraView {
    layers: RenderXXLayersXX,
}

impl CameraView {
    /// Makes a new `CameraView` with no visible [`RenderLayer`].
    pub fn empty() -> Self {
        Self {
            layers: RenderXXLayersXX::empty(),
        }
    }

    /// Adds a [`RenderLayer`].
    ///
    /// See [`RenderXXLayersXX::add`].
    pub fn add(&mut self, layer: impl Into<RenderLayer>) -> &mut Self {
        self.layers.add(layer);
        self
    }

    /// Removes a [`RenderLayer`].
    ///
    /// See [`RenderXXLayersXX::remove`].
    pub fn remove(&mut self, layer: impl Into<RenderLayer>) -> &mut Self {
        self.layers.remove(layer);
        self
    }

    /// Clears all stored render layers without deallocating.
    pub fn clear(&mut self) {
        self.layers.clear();
    }

    /// Returns an iterator over [`RenderLayer`].
    pub fn iter_layers(&self) -> impl Iterator<Item = RenderLayer> + '_ {
        self.layers.iter()
    }

    /// Returns `true` if the specified render layer is included in this `CameraView`.
    pub fn contains_layer(&self, layer: impl Into<RenderLayer>) -> bool {
        self.layers.contains(layer)
    }

    /// Returns `true` if the entity with the specified [`RenderGroups`] is visible
    /// to the `camera` that has this `CameraView`.
    ///
    /// Checks both camera affiliation and [`RenderXXLayersXX`] intersection.
    pub fn entity_is_visible(&self, camera: Entity, groups: &RenderGroups) -> bool {
        if Some(camera) == groups.camera {
            return true;
        }
        self.layers.intersects(&groups.layers)
    }

    /// Converts the internal [`RenderXXLayersXX`] into a [`RenderGroups`] affiliated
    /// with the camera that has this `CameraView`.
    pub fn get_groups(&self, camera: Entity) -> RenderGroups {
        let mut groups = RenderGroups::from(self.layers.clone());
        groups.set_camera(camera);
        groups
    }
}

impl From<RenderLayer> for CameraView {
    /// Makes a new `CameraView` from a specific [`RenderLayer`].
    fn from(layer: RenderLayer) -> Self {
        Self {
            layers: RenderXXLayersXX::from(layer),
        }
    }
}

impl Default for CameraView {
    /// Equivalent to `Self::from(DEFAULT_RENDER_LAYER)`.
    fn default() -> Self {
        Self::from(DEFAULT_RENDER_LAYER)
    }
}

#[cfg(test)]
mod rendering_mask_tests {
    use super::{RenderLayer, RenderXXLayersXX, DEFAULT_RENDER_LAYER};
    use smallvec::SmallVec;

    #[test]
    fn rendering_mask_sanity() {
        assert_eq!(
            RenderXXLayersXX::default().num_layers(),
            1,
            "default layer contains only one layer"
        );
        assert!(
            RenderXXLayersXX::default().contains(DEFAULT_RENDER_LAYER),
            "default layer contains default"
        );
        assert_eq!(
            RenderXXLayersXX::from(RenderLayer(1)).num_layers(),
            1,
            "from contains 1 layer"
        );
        assert!(
            RenderXXLayersXX::from(RenderLayer(1)).contains(RenderLayer(1)),
            "contains is accurate"
        );
        assert!(
            !RenderXXLayersXX::from(RenderLayer(1)).contains(RenderLayer(2)),
            "contains fails when expected"
        );

        assert_eq!(
            RenderXXLayersXX::from(RenderLayer(0)).add(1).layers[0],
            3,
            "layer 0 + 1 is mask 3"
        );
        assert_eq!(
            RenderXXLayersXX::from(RenderLayer(0)).add(1).remove(0).layers[0],
            2,
            "layer 0 + 1 - 0 is mask 2"
        );
        assert!(
            RenderXXLayersXX::from(RenderLayer(1)).intersects(&RenderXXLayersXX::from(RenderLayer(1))),
            "layers match like layers"
        );
        assert!(
            RenderXXLayersXX::from(RenderLayer(0)).intersects(&RenderXXLayersXX {
                layers: SmallVec::from_slice(&[1])
            }),
            "a layer of 0 means the mask is just 1 bit"
        );

        assert!(
            RenderXXLayersXX::from(RenderLayer(0))
                .add(3)
                .intersects(&RenderXXLayersXX::from(RenderLayer(3))),
            "a mask will match another mask containing any similar layers"
        );

        assert!(
            RenderXXLayersXX::default().intersects(&RenderXXLayersXX::default()),
            "default masks match each other"
        );

        assert!(
            !RenderXXLayersXX::from(RenderLayer(0)).intersects(&RenderXXLayersXX::from(RenderLayer(1))),
            "masks with differing layers do not match"
        );
        assert!(
            !RenderXXLayersXX::empty().intersects(&RenderXXLayersXX::empty()),
            "empty masks don't match"
        );
        assert_eq!(
            RenderXXLayersXX::from_layers(&[0, 2, 16, 30])
                .iter()
                .collect::<Vec<_>>(),
            vec![
                RenderLayer(0),
                RenderLayer(2),
                RenderLayer(16),
                RenderLayer(30)
            ],
            "from and get_layers should roundtrip"
        );
        assert_eq!(
            format!("{:?}", RenderXXLayersXX::from_layers(&[0, 1, 2, 3])).as_str(),
            "RenderXXLayersXX([0, 1, 2, 3])",
            "Debug instance shows layers"
        );
        assert_eq!(
            RenderXXLayersXX::from_layers(&[0, 1, 2]),
            <RenderXXLayersXX as FromIterator<usize>>::from_iter(vec![0, 1, 2]),
            "from_layers and from_iter are equivalent"
        );
    }
}
