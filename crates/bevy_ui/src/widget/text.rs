use crate::{
    ContentSize, DefaultUiCamera, FixedMeasure, Measure, MeasureArgs, Node, NodeMeasure,
    TargetCamera, UiScale,
};
use bevy_asset::Assets;
use bevy_ecs::{
    entity::{Entity, EntityHashMap},
    prelude::{Component, DetectChanges},
    query::With,
    reflect::ReflectComponent,
    system::{Local, Query, Res, ResMut},
    world::{Mut, Ref},
};
use bevy_math::Vec2;
use bevy_reflect::{std_traits::ReflectDefault, Reflect};
use bevy_render::{camera::Camera, texture::Image};
use bevy_sprite::TextureAtlasLayout;
use bevy_text::{
    scale_value, CosmicBuffer, CosmicFontSystem, Font, FontAtlasSets, JustifyText, LineBreak,
    SwashCache, Text, TextBounds, TextError, TextLayoutInfo, TextMeasureInfo, TextPipeline,
    YAxisOrientation,
};
use bevy_utils::{tracing::error, Entry};
use taffy::style::AvailableSpace;

/// UI text system flags.
///
/// Used internally by [`measure_text_system`] and [`text_system`] to schedule text for processing.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct TextNodeFlags
{
    /// If set a new measure function for the text node will be created.
    needs_measure_fn: bool,
    /// If set the text will be recomputed
    needs_recompute: bool,
}

impl Default for TextNodeFlags {
    fn default() -> Self {
        Self {
            needs_measure_fn: true,
            needs_recompute: true,
        }
    }
}

/// The top-level UI text component.
///
/// If a string is specified, it behaves as if it has a "first" TextSpan child.
#[derive(Component)]
#[require(
    TextBlock,
    TextStyle,
    Node,
    Style, // TODO: Remove when Node uses required components.
    ContentSize, // TODO: Remove when Node uses required components.
    FocusPolicy, // TODO: Remove when Node uses required components.
    ZIndex, // TODO: Remove when Node uses required components.
    BackgroundColor, // TODO: Remove when Node uses required components.
    TextNodeFlags,
    Visibility, // TODO: Remove when Node uses required components.
    Transform // TODO: Remove when Node uses required components.
)]
pub struct TextNEW(pub String);

/// A span of text in a tree of spans under an entity with [`Text`].
///
/// Spans are collected in hierarchy traversal order into a [`ComputedTextBlock`] for layout.
#[derive(Component)]
#[require(TextStyle, GhostNode, Visibility = Visibility::Hidden)]
pub struct TextSpan(pub String);



#[cfg(feature = "bevy_text")]
/// A UI node that is text
///
/// The positioning of this node is controlled by the UI layout system. If you need manual control,
/// use [`Text2dBundle`](bevy_text::Text2dBundle).
#[derive(Bundle, Debug, Default)]
pub struct TextBundle {
    /// Describes the logical size of the node
    pub node: Node,
    /// Styles which control the layout (size and position) of the node and its children
    /// In some cases these styles also affect how the node drawn/painted.
    pub style: Style,
    /// Contains the text of the node
    pub text: Text,
    /// Cached cosmic buffer for layout
    pub buffer: CosmicBuffer,
    /// Text layout information
    pub text_layout_info: TextLayoutInfo,
    /// Text system flags
    pub text_flags: TextFlags,
    /// The calculated size based on the given image
    pub calculated_size: ContentSize,
    /// Whether this node should block interaction with lower nodes
    pub focus_policy: FocusPolicy,
    /// The transform of the node
    ///
    /// This component is automatically managed by the UI layout system.
    /// To alter the position of the `TextBundle`, use the properties of the [`Style`] component.
    pub transform: Transform,
    /// The global transform of the node
    ///
    /// This component is automatically updated by the [`TransformPropagate`](`bevy_transform::TransformSystem::TransformPropagate`) systems.
    pub global_transform: GlobalTransform,
    /// Describes the visibility properties of the node
    pub visibility: Visibility,
    /// Inherited visibility of an entity.
    pub inherited_visibility: InheritedVisibility,
    /// Algorithmically-computed indication of whether an entity is visible and should be extracted for rendering
    pub view_visibility: ViewVisibility,
    /// Indicates the depth at which the node should appear in the UI
    pub z_index: ZIndex,
    /// The background color that will fill the containing node
    pub background_color: BackgroundColor,
}

#[cfg(feature = "bevy_text")]
impl TextBundle {
    /// Create a [`TextBundle`] from a single section.
    ///
    /// See [`Text::from_section`] for usage.
    pub fn from_section(value: impl Into<String>, style: TextStyle) -> Self {
        Self {
            text: Text::from_section(value, style),
            ..Default::default()
        }
    }

    /// Create a [`TextBundle`] from a list of sections.
    ///
    /// See [`Text::from_sections`] for usage.
    pub fn from_sections(sections: impl IntoIterator<Item = TextSection>) -> Self {
        Self {
            text: Text::from_sections(sections),
            ..Default::default()
        }
    }

    /// Returns this [`TextBundle`] with a new [`JustifyText`] on [`Text`].
    pub const fn with_text_justify(mut self, justify: JustifyText) -> Self {
        self.text.justify = justify;
        self
    }

    /// Returns this [`TextBundle`] with a new [`Style`].
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Returns this [`TextBundle`] with a new [`BackgroundColor`].
    pub const fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = BackgroundColor(color);
        self
    }

    /// Returns this [`TextBundle`] with soft wrapping disabled.
    /// Hard wrapping, where text contains an explicit linebreak such as the escape sequence `\n`, will still occur.
    pub const fn with_no_wrap(mut self) -> Self {
        self.text.linebreak_behavior = LineBreak::NoWrap;
        self
    }
}

#[cfg(feature = "bevy_text")]
impl<I> From<I> for TextBundle
where
    I: Into<TextSection>,
{
    fn from(value: I) -> Self {
        Self::from_sections(vec![value.into()])
    }
}



/// Text measurement for UI layout. See [`NodeMeasure`].
pub struct TextMeasure {
    pub info: TextMeasureInfo,
}

impl TextMeasure {
    /// Checks if the cosmic text buffer is needed for measuring the text.
    pub fn needs_buffer(height: Option<f32>, available_width: AvailableSpace) -> bool {
        height.is_none() && matches!(available_width, AvailableSpace::Definite(_))
    }
}

impl Measure for TextMeasure {
    fn measure(&mut self, measure_args: MeasureArgs, _style: &taffy::Style) -> Vec2 {
        let MeasureArgs {
            width,
            height,
            available_width,
            buffer,
            font_system,
            ..
        } = measure_args;
        let x = width.unwrap_or_else(|| match available_width {
            AvailableSpace::Definite(x) => {
                // It is possible for the "min content width" to be larger than
                // the "max content width" when soft-wrapping right-aligned text
                // and possibly other situations.

                x.max(self.info.min.x).min(self.info.max.x)
            }
            AvailableSpace::MinContent => self.info.min.x,
            AvailableSpace::MaxContent => self.info.max.x,
        });

        height
            .map_or_else(
                || match available_width {
                    AvailableSpace::Definite(_) => {
                        if let Some(buffer) = buffer {
                            self.info.compute_size(
                                TextBounds::new_horizontal(x),
                                buffer,
                                font_system,
                            )
                        } else {
                            error!("text measure failed, buffer is missing");
                            Vec2::default()
                        }
                    }
                    AvailableSpace::MinContent => Vec2::new(x, self.info.min.y),
                    AvailableSpace::MaxContent => Vec2::new(x, self.info.max.y),
                },
                |y| Vec2::new(x, y),
            )
            .ceil()
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn create_text_measure(
    entity: Entity,
    fonts: &Assets<Font>,
    scale_factor: f64,
    text: Ref<Text>,
    text_pipeline: &mut TextPipeline,
    mut content_size: Mut<ContentSize>,
    mut text_flags: Mut<TextFlags>,
    buffer: &mut CosmicBuffer,
    text_alignment: JustifyText,
    font_system: &mut CosmicFontSystem,
) {
    match text_pipeline.create_text_measure(
        entity,
        fonts,
        &text.sections,
        scale_factor,
        text.linebreak,
        buffer,
        text_alignment,
        font_system,
    ) {
        Ok(measure) => {
            if text.linebreak == LineBreak::NoWrap {
                content_size.set(NodeMeasure::Fixed(FixedMeasure { size: measure.max }));
            } else {
                content_size.set(NodeMeasure::Text(TextMeasure { info: measure }));
            }

            // Text measure func created successfully, so set `TextFlags` to schedule a recompute
            text_flags.needs_measure_fn = false;
            text_flags.needs_recompute = true;
        }
        Err(TextError::NoSuchFont) => {
            // Try again next frame
            text_flags.needs_measure_fn = true;
        }
        Err(e @ (TextError::FailedToAddGlyph(_) | TextError::FailedToGetGlyphImage(_))) => {
            panic!("Fatal error when processing text: {e}.");
        }
    };
}

/// Generates a new [`Measure`] for a text node on changes to its [`Text`] component.
///
/// A `Measure` is used by the UI's layout algorithm to determine the appropriate amount of space
/// to provide for the text given the fonts, the text itself and the constraints of the layout.
///
/// * Measures are regenerated if the target camera's scale factor (or primary window if no specific target) or [`UiScale`] is changed.
/// * Changes that only modify the colors of a `Text` do not require a new `Measure`. This system
///     is only able to detect that a `Text` component has changed and will regenerate the `Measure` on
///     color changes. This can be expensive, particularly for large blocks of text, and the [`bypass_change_detection`](bevy_ecs::change_detection::DetectChangesMut::bypass_change_detection)
///     method should be called when only changing the `Text`'s colors.
#[allow(clippy::too_many_arguments)]
pub fn measure_text_system(
    mut scale_factors_buffer: Local<EntityHashMap<f32>>,
    mut last_scale_factors: Local<EntityHashMap<f32>>,
    fonts: Res<Assets<Font>>,
    camera_query: Query<(Entity, &Camera)>,
    default_ui_camera: DefaultUiCamera,
    ui_scale: Res<UiScale>,
    mut text_query: Query<
        (
            Entity,
            Ref<Text>,
            &mut ContentSize,
            &mut TextFlags,
            Option<&TargetCamera>,
            &mut CosmicBuffer,
        ),
        With<Node>,
    >,
    mut text_pipeline: ResMut<TextPipeline>,
    mut font_system: ResMut<CosmicFontSystem>,
) {
    scale_factors_buffer.clear();

    for (entity, text, content_size, text_flags, camera, mut buffer) in &mut text_query {
        let Some(camera_entity) = camera.map(TargetCamera::entity).or(default_ui_camera.get())
        else {
            continue;
        };
        let scale_factor = match scale_factors_buffer.entry(camera_entity) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => *entry.insert(
                camera_query
                    .get(camera_entity)
                    .ok()
                    .and_then(|(_, c)| c.target_scaling_factor())
                    .unwrap_or(1.0)
                    * ui_scale.0,
            ),
        };
        if last_scale_factors.get(&camera_entity) != Some(&scale_factor)
            || text.is_changed()  // TODO: get needs_recompute from ComputedTextBlock
            || text_flags.needs_measure_fn
            || content_size.is_added()
        {
            let text_alignment = text.justify;
            create_text_measure(
                entity,
                &fonts,
                scale_factor.into(),
                text,
                &mut text_pipeline,
                content_size,
                text_flags,
                buffer.as_mut(),
                text_alignment,
                &mut font_system,
            );
        }
    }
    core::mem::swap(&mut *last_scale_factors, &mut *scale_factors_buffer);
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn queue_text(
    fonts: &Assets<Font>,
    text_pipeline: &mut TextPipeline,
    font_atlas_sets: &mut FontAtlasSets,
    texture_atlases: &mut Assets<TextureAtlasLayout>,
    textures: &mut Assets<Image>,
    scale_factor: f32,
    inverse_scale_factor: f32,
    text: &Text,
    node: Ref<Node>,
    mut text_flags: Mut<TextFlags>,
    text_layout_info: Mut<TextLayoutInfo>,
    buffer: &mut CosmicBuffer,
    font_system: &mut CosmicFontSystem,
    swash_cache: &mut SwashCache,
) {
    // Skip the text node if it is waiting for a new measure func
    if !text_flags.needs_new_measure_func {
        let physical_node_size = if text.linebreak == LineBreak::NoWrap {
            // With `NoWrap` set, no constraints are placed on the width of the text.
            TextBounds::UNBOUNDED
        } else {
            // `scale_factor` is already multiplied by `UiScale`
            TextBounds::new(
                node.unrounded_size.x * scale_factor,
                node.unrounded_size.y * scale_factor,
            )
        };

        let text_layout_info = text_layout_info.into_inner();
        match text_pipeline.queue_text(
            text_layout_info,
            fonts,
            &text.sections,
            scale_factor.into(),
            text.justify,
            text.linebreak,
            text.font_smoothing,
            physical_node_size,
            font_atlas_sets,
            texture_atlases,
            textures,
            YAxisOrientation::TopToBottom,
            buffer,
            font_system,
            swash_cache,
        ) {
            Err(TextError::NoSuchFont) => {
                // There was an error processing the text layout, try again next frame
                text_flags.needs_recompute = true;
            }
            Err(e @ (TextError::FailedToAddGlyph(_) | TextError::FailedToGetGlyphImage(_))) => {
                panic!("Fatal error when processing text: {e}.");
            }
            Ok(()) => {
                text_layout_info.size.x =
                    scale_value(text_layout_info.size.x, inverse_scale_factor);
                text_layout_info.size.y =
                    scale_value(text_layout_info.size.y, inverse_scale_factor);
                text_flags.needs_recompute = false;
            }
        }
    }
}

/// Updates the layout and size information for a UI text node on changes to the size value of its [`Node`] component,
/// or when the `needs_recompute` field of [`TextFlags`] is set to true.
/// This information is computed by the [`TextPipeline`] and then stored in [`TextLayoutInfo`].
///
/// ## World Resources
///
/// [`ResMut<Assets<Image>>`](Assets<Image>) -- This system only adds new [`Image`] assets.
/// It does not modify or observe existing ones. The exception is when adding new glyphs to a [`bevy_text::FontAtlas`].
#[allow(clippy::too_many_arguments)]
pub fn text_system(
    mut textures: ResMut<Assets<Image>>,
    mut scale_factors_buffer: Local<EntityHashMap<f32>>,
    mut last_scale_factors: Local<EntityHashMap<f32>>,
    fonts: Res<Assets<Font>>,
    camera_query: Query<(Entity, &Camera)>,
    default_ui_camera: DefaultUiCamera,
    ui_scale: Res<UiScale>,
    mut texture_atlases: ResMut<Assets<TextureAtlasLayout>>,
    mut font_atlas_sets: ResMut<FontAtlasSets>,
    mut text_pipeline: ResMut<TextPipeline>,
    mut text_query: Query<(
        Ref<Node>,
        &Text,
        &mut TextLayoutInfo,
        &mut TextFlags,
        Option<&TargetCamera>,
        &mut CosmicBuffer,
    )>,
    mut font_system: ResMut<CosmicFontSystem>,
    mut swash_cache: ResMut<SwashCache>,
) {
    scale_factors_buffer.clear();

    for (node, text, text_layout_info, text_flags, camera, mut buffer) in &mut text_query {
        let Some(camera_entity) = camera.map(TargetCamera::entity).or(default_ui_camera.get())
        else {
            continue;
        };
        let scale_factor = match scale_factors_buffer.entry(camera_entity) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => *entry.insert(
                camera_query
                    .get(camera_entity)
                    .ok()
                    .and_then(|(_, c)| c.target_scaling_factor())
                    .unwrap_or(1.0)
                    * ui_scale.0,
            ),
        };
        let inverse_scale_factor = scale_factor.recip();

        if last_scale_factors.get(&camera_entity) != Some(&scale_factor)
            || node.is_changed()
            || text_flags.needs_recompute
        {
            queue_text(
                &fonts,
                &mut text_pipeline,
                &mut font_atlas_sets,
                &mut texture_atlases,
                &mut textures,
                scale_factor,
                inverse_scale_factor,
                text,
                node,
                text_flags,
                text_layout_info,
                buffer.as_mut(),
                &mut font_system,
                &mut swash_cache,
            );
        }
    }
    core::mem::swap(&mut *last_scale_factors, &mut *scale_factors_buffer);
}
