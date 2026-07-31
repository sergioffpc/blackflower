# SPDX-License-Identifier: GPL-2.0-or-later

"""Blender integration for Blackflower-owned glTF extras."""

import bpy
from bpy.props import (
    BoolProperty,
    EnumProperty,
    FloatProperty,
    PointerProperty,
    StringProperty,
)
from bpy.types import Panel, PropertyGroup

from .metadata import (
    MetadataError,
    build_animation_metadata,
    build_material_metadata,
    build_node_metadata,
    merge_extras,
)


ADDON_VERSION = (0, 1, 0)

bl_info = {
    "name": "Blackflower glTF Metadata",
    "category": "Import-Export",
    "version": ADDON_VERSION,
    "blender": (4, 2, 0),
    "location": "File > Export > glTF 2.0 and Object Properties",
    "description": "Export typed Blackflower metadata in glTF extras",
}

_EXPORT_PANEL_KEY = "Blackflower glTF Metadata"
_SUPPORTED_ANIMATION_MODES = {None, "ACTIONS"}
_SUPPORTED_MERGE_MODES = {None, "NONE", "ACTION"}


class BlackflowerExportSettings(PropertyGroup):
    """Per-scene switch displayed in the glTF exporter."""

    enabled: BoolProperty(
        name="Blackflower Metadata",
        description="Export validated Blackflower markers and node metadata",
        default=True,
    )


class BlackflowerNodeMetadata(PropertyGroup):
    """Typed metadata attached to an exported Blender object."""

    enabled: BoolProperty(
        name="Blackflower Node",
        description="Attach typed Blackflower metadata to this glTF node",
        default=False,
    )
    kind: StringProperty(
        name="Kind",
        description="Stable lower_snake_case domain type, such as spawn_point",
        default="",
        maxlen=64,
    )
    identifier: StringProperty(
        name="ID",
        description="Optional stable identifier within the source asset",
        default="",
        maxlen=128,
    )
    navigation_role: EnumProperty(
        name="Navigation Role",
        description="How this object participates in Recast cooking",
        items=(
            ("none", "None", "Do not export navigation metadata"),
            ("surface", "Surface", "Rasterize triangles as an authored area"),
            ("obstacle", "Obstacle", "Rasterize triangles as blocked geometry"),
            (
                "off_mesh_link",
                "Off-mesh Link",
                "Use the first two mesh vertices as connection endpoints",
            ),
        ),
        default="none",
    )
    navigation_area_key: StringProperty(
        name="Area",
        description="Area key declared in the navigation asset.toml",
        default="",
        maxlen=64,
    )
    navigation_direction: EnumProperty(
        name="Direction",
        items=(
            ("bidirectional", "Bidirectional", "Allow travel both ways"),
            ("one_way", "One Way", "Travel from the first endpoint to the second"),
        ),
        default="bidirectional",
    )
    navigation_radius: FloatProperty(
        name="Radius",
        description="Off-mesh endpoint matching radius in world units",
        default=0.5,
        min=0.0001,
    )
    acoustics_kind: EnumProperty(
        name="Acoustic Role",
        description="How this object participates in acoustic cooking",
        items=(
            ("none", "None", "Do not export acoustic metadata"),
            ("geometry", "Geometry", "Classify mesh geometry for acoustics"),
            ("zone", "Zone", "Identify an acoustic zone"),
            (
                "zone_volume",
                "Zone Volume",
                "Bound a Stage 9 acoustic zone for authoritative broad phase",
            ),
            ("portal", "Portal", "Connect two Stage 9 acoustic zones"),
            (
                "probe_volume",
                "Probe Volume",
                "Bound automatic probe generation for one zone",
            ),
        ),
        default="none",
    )
    acoustic_geometry_class: EnumProperty(
        name="Geometry Class",
        items=(
            ("static", "Static", "Include in the Stage 8 static scene"),
            (
                "dynamic_rigid",
                "Dynamic Rigid",
                "Reserve rigid movable geometry for Stage 9",
            ),
            (
                "dynamic_state",
                "Dynamic State",
                "Reserve state-dependent geometry for Stage 9",
            ),
            ("ignored", "Ignored", "Exclude geometry from acoustic cooking"),
        ),
        default="static",
    )
    acoustic_zone: StringProperty(
        name="Zone",
        description="Stable acoustic zone ID containing this probe volume",
        default="",
        maxlen=128,
    )
    acoustic_zone_a: StringProperty(
        name="Zone A",
        description="First adjacent acoustic zone-volume ID",
        default="",
        maxlen=128,
    )
    acoustic_zone_b: StringProperty(
        name="Zone B",
        description="Second adjacent acoustic zone-volume ID",
        default="",
        maxlen=128,
    )


class BlackflowerMaterialMetadata(PropertyGroup):
    """Acoustic material asset attached to a Blender material."""

    acoustic_material: StringProperty(
        name="Acoustic Material",
        description="Portable asset ID with absorption, scattering, and transmission",
        default="",
        maxlen=255,
    )


class BlackflowerAnimationMetadata(PropertyGroup):
    """Typed cooking policy attached to a Blender Action."""

    looping: BoolProperty(
        name="Loop",
        description="Play this animation as a loop",
        default=False,
    )
    additive_enabled: BoolProperty(
        name="Additive",
        description="Cook this Action as an additive animation",
        default=False,
    )
    additive_reference: EnumProperty(
        name="Additive Reference",
        items=(
            ("animation", "Animation", "Use the first keyframe"),
            ("skeleton", "Skeleton", "Use the skeleton rest pose"),
        ),
        default="animation",
    )
    root_motion_enabled: BoolProperty(
        name="Root Motion",
        description="Extract a runtime root-motion track",
        default=False,
    )
    root_motion_joint: StringProperty(
        name="Joint",
        description="Exact skeleton joint name used for root motion",
        default="Root",
        maxlen=128,
    )
    translation_x: BoolProperty(name="Translation X", default=True)
    translation_y: BoolProperty(name="Translation Y", default=False)
    translation_z: BoolProperty(name="Translation Z", default=True)
    rotation_x: BoolProperty(name="Rotation X", default=False)
    rotation_y: BoolProperty(name="Rotation Y", default=True)
    rotation_z: BoolProperty(name="Rotation Z", default=False)
    root_motion_reference: EnumProperty(
        name="Root Reference",
        items=(
            ("absolute", "Absolute", "Use the source transform"),
            ("skeleton", "Skeleton", "Use the skeleton rest transform"),
            ("animation", "Animation", "Use the first animation keyframe"),
        ),
        default="skeleton",
    )
    remove_from_pose: BoolProperty(
        name="Remove from Pose",
        description="Remove extracted motion from the skeletal pose",
        default=True,
    )
    loop_correction: BoolProperty(
        name="Loop Correction",
        description="Correct the final root key for a seamless loop",
        default=False,
    )


class BLACKFLOWER_PT_animation_metadata(Panel):
    """Action metadata panel in the Dope Sheet sidebar."""

    bl_label = "Blackflower Animation"
    bl_idname = "BLACKFLOWER_PT_animation_metadata"
    bl_space_type = "DOPESHEET_EDITOR"
    bl_region_type = "UI"
    bl_category = "Blackflower"

    @classmethod
    def poll(cls, context):
        return _active_action(context) is not None

    def draw(self, context):
        properties = _active_action(context).blackflower_animation_metadata
        layout = self.layout
        layout.use_property_split = True
        layout.prop(properties, "looping")
        layout.prop(properties, "additive_enabled")
        additive = layout.column()
        additive.enabled = properties.additive_enabled
        additive.prop(properties, "additive_reference")

        layout.prop(properties, "root_motion_enabled")
        motion = layout.column()
        motion.enabled = properties.root_motion_enabled
        motion.prop(properties, "root_motion_joint")
        motion.prop(properties, "root_motion_reference")
        motion.prop(properties, "remove_from_pose")
        motion.prop(properties, "loop_correction")
        translation = motion.row(align=True)
        translation.label(text="Translation")
        translation.prop(properties, "translation_x", text="X", toggle=True)
        translation.prop(properties, "translation_y", text="Y", toggle=True)
        translation.prop(properties, "translation_z", text="Z", toggle=True)
        rotation = motion.row(align=True)
        rotation.label(text="Rotation")
        rotation.prop(properties, "rotation_x", text="X", toggle=True)
        rotation.prop(properties, "rotation_y", text="Y", toggle=True)
        rotation.prop(properties, "rotation_z", text="Z", toggle=True)


class BLACKFLOWER_PT_node_metadata(Panel):
    """Object Properties panel for model and level node metadata."""

    bl_label = "Blackflower Metadata"
    bl_idname = "BLACKFLOWER_PT_node_metadata"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_context = "object"

    @classmethod
    def poll(cls, context):
        return context.object is not None

    def draw(self, context):
        layout = self.layout
        properties = context.object.blackflower_node_metadata
        layout.use_property_split = True
        layout.prop(properties, "enabled")

        navigation_role = properties.navigation_role
        acoustics_kind = getattr(properties, "acoustics_kind", "none")
        fields = layout.column()
        fields.enabled = (
            properties.enabled
            or navigation_role != "none"
            or acoustics_kind != "none"
        )
        fields.prop(properties, "kind")
        fields.prop(properties, "identifier")
        if properties.enabled and not properties.kind:
            fields.label(text="Kind is required for export", icon="ERROR")
        layout.separator()
        layout.prop(properties, "navigation_role")
        navigation = layout.column()
        navigation.enabled = navigation_role != "none"
        if navigation_role in {"surface", "off_mesh_link"}:
            navigation.prop(properties, "navigation_area_key")
        if navigation_role == "off_mesh_link":
            navigation.prop(properties, "navigation_direction")
            navigation.prop(properties, "navigation_radius")
        if navigation_role != "none" and not properties.identifier:
            navigation.label(text="Stable ID is required for navigation", icon="ERROR")
        layout.separator()
        layout.prop(properties, "acoustics_kind")
        acoustics = layout.column()
        acoustics.enabled = acoustics_kind != "none"
        if acoustics_kind == "geometry":
            acoustics.prop(properties, "acoustic_geometry_class")
        if acoustics_kind == "probe_volume":
            acoustics.prop(properties, "acoustic_zone")
        if acoustics_kind == "portal":
            acoustics.prop(properties, "acoustic_zone_a")
            acoustics.prop(properties, "acoustic_zone_b")
        if acoustics_kind != "none" and not properties.identifier:
            acoustics.label(text="Stable ID is required for acoustics", icon="ERROR")
        if acoustics_kind != "none":
            expected_kind = f"acoustic_{acoustics_kind}"
            if properties.kind and properties.kind != expected_kind:
                acoustics.label(text=f"Kind must be {expected_kind}", icon="ERROR")


class BLACKFLOWER_PT_material_metadata(Panel):
    """Material Properties panel for acoustic material mapping."""

    bl_label = "Blackflower Acoustics"
    bl_idname = "BLACKFLOWER_PT_material_metadata"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_context = "material"

    @classmethod
    def poll(cls, context):
        return context.material is not None

    def draw(self, context):
        layout = self.layout
        layout.use_property_split = True
        layout.prop(
            context.material.blackflower_material_metadata,
            "acoustic_material",
        )


def draw_export(context, layout):
    """Draw the add-on section in Blender's glTF exporter."""

    properties = context.scene.blackflower_gltf_export
    header, body = layout.panel(
        "GLTF_blackflower_metadata_exporter",
        default_closed=False,
    )
    header.use_property_split = False
    header.prop(properties, "enabled")
    if body is not None and properties.enabled:
        body.label(text="Action policy and Pose Markers become animation metadata.")
        body.label(text="Object metadata becomes typed node extras.")
        body.label(text="Material mappings and probe volumes become acoustic extras.")


# The glTF exporter discovers this exact class name.
class glTF2ExportUserExtension:
    """glTF-Blender-IO user extension."""

    def __init__(self):
        self.properties = bpy.context.scene.blackflower_gltf_export
        self.is_critical = True

    def animation_action_hook(
        self,
        gltf2_animation,
        blender_object,
        blender_action_data,
        export_settings,
    ):
        """Attach action-local Pose Markers to one glTF animation."""

        if not self.properties.enabled:
            return

        action = getattr(blender_action_data, "action", None)
        if action is None:
            return
        self._validate_animation_export_mode(action.name, export_settings)

        frame_start, frame_end, target_ids = _action_export_range(
            blender_object,
            action,
            export_settings,
        )
        origin = _action_time_origin(
            action.name,
            frame_start,
            target_ids,
            export_settings,
        )
        metadata = build_animation_metadata(
            ((marker.name, marker.frame) for marker in action.pose_markers),
            frame_start=frame_start,
            frame_end=frame_end,
            frames_per_second=_frames_per_second(export_settings),
            time_origin_frame=origin,
            **_animation_settings(action),
        )
        gltf2_animation.extras = merge_extras(gltf2_animation.extras, metadata)

    def gather_node_hook(self, gltf2_node, blender_object, export_settings):
        """Attach typed model/level metadata to one glTF node."""

        del export_settings
        if not self.properties.enabled or not isinstance(
            blender_object, bpy.types.Object
        ):
            return

        properties = blender_object.blackflower_node_metadata
        navigation_role = getattr(properties, "navigation_role", "none")
        acoustics_kind = getattr(properties, "acoustics_kind", "none")
        if (
            not properties.enabled
            and navigation_role == "none"
            and acoustics_kind == "none"
        ):
            return
        kind = properties.kind
        if not kind and acoustics_kind != "none":
            kind = f"acoustic_{acoustics_kind}"
        if not kind and navigation_role != "none":
            kind = f"navigation_{navigation_role}"
        metadata = build_node_metadata(
            kind,
            properties.identifier,
            navigation_role=navigation_role,
            area_key=getattr(properties, "navigation_area_key", ""),
            direction=getattr(
                properties,
                "navigation_direction",
                "bidirectional",
            ),
            radius=getattr(properties, "navigation_radius", 0.0),
            acoustics_kind=acoustics_kind,
            geometry_class=getattr(
                properties,
                "acoustic_geometry_class",
                "static",
            ),
            acoustic_zone=getattr(properties, "acoustic_zone", ""),
            acoustic_zone_a=getattr(properties, "acoustic_zone_a", ""),
            acoustic_zone_b=getattr(properties, "acoustic_zone_b", ""),
        )
        gltf2_node.extras = merge_extras(gltf2_node.extras, metadata)

    def gather_material_hook(
        self,
        gltf2_material,
        blender_material,
        export_settings,
    ):
        """Attach a portable acoustic-material asset ID."""

        del export_settings
        if not self.properties.enabled:
            return
        properties = getattr(
            blender_material,
            "blackflower_material_metadata",
            None,
        )
        if properties is None:
            return
        metadata = build_material_metadata(properties.acoustic_material)
        gltf2_material.extras = merge_extras(gltf2_material.extras, metadata)

    def merge_animation_extensions_hook(
        self,
        gltf2_animation_source,
        gltf2_animation_destination,
        export_settings,
    ):
        """Reject incompatible marker tracks before glTF merges extras."""

        del export_settings
        if not self.properties.enabled:
            return
        source = _blackflower_extra(gltf2_animation_source)
        destination = _blackflower_extra(gltf2_animation_destination)
        if source is not None and destination is not None and source != destination:
            raise MetadataError(
                "the glTF exporter is merging animations with different "
                "Blackflower marker tracks; use Actions mode and merge by "
                "Action or disable animation merging"
            )

    @staticmethod
    def _validate_animation_export_mode(action_name, export_settings):
        mode = export_settings.get("gltf_animation_mode")
        if mode not in _SUPPORTED_ANIMATION_MODES:
            raise MetadataError(
                f"action `{action_name}` has Blackflower metadata, but animation mode "
                f"`{mode}` cannot preserve their action-local timeline; use "
                "Actions mode"
            )
        merge = export_settings.get("gltf_merge_animation")
        if merge not in _SUPPORTED_MERGE_MODES:
            raise MetadataError(
                f"action `{action_name}` has Blackflower metadata, but merge mode "
                f"`{merge}` can combine unrelated timelines; use None or Action"
            )


def _blackflower_extra(animation):
    extras = getattr(animation, "extras", None)
    return extras.get("blackflower") if isinstance(extras, dict) else None


def _active_action(context):
    animation_data = getattr(getattr(context, "object", None), "animation_data", None)
    return getattr(animation_data, "action", None)


def _animation_settings(action):
    properties = getattr(action, "blackflower_animation_metadata", None)
    if properties is None:
        return {}
    return {
        "looping": properties.looping,
        "additive_enabled": properties.additive_enabled,
        "additive_reference": properties.additive_reference,
        "root_motion_enabled": properties.root_motion_enabled,
        "root_motion_joint": properties.root_motion_joint,
        "translation_axes": _selected_axes(
            properties.translation_x,
            properties.translation_y,
            properties.translation_z,
        ),
        "rotation_axes": _selected_axes(
            properties.rotation_x,
            properties.rotation_y,
            properties.rotation_z,
        ),
        "root_motion_reference": properties.root_motion_reference,
        "remove_from_pose": properties.remove_from_pose,
        "loop_correction": properties.loop_correction,
    }


def _selected_axes(x, y, z):
    return [
        axis
        for axis, enabled in (("x", x), ("y", y), ("z", z))
        if enabled
    ]


def _frames_per_second(export_settings):
    configured = export_settings.get("gltf_fps")
    if configured is not None:
        return configured
    render = bpy.context.scene.render
    return render.fps / render.fps_base


def _action_export_range(blender_object, action, export_settings):
    target_ids = _target_ids(blender_object, export_settings)
    candidates = _range_candidates(action.name, target_ids, export_settings)
    if not candidates:
        candidates = _range_candidates(action.name, None, export_settings)
    if candidates:
        distinct = {(start, end) for _, start, end in candidates}
        if len(distinct) != 1:
            raise MetadataError(
                f"action `{action.name}` is exported with different frame "
                "ranges and cannot have one unambiguous marker track"
            )
        frame_start, frame_end = distinct.pop()
        return frame_start, frame_end, [candidate[0] for candidate in candidates]

    # Compatibility fallback for exporter versions that do not expose ranges.
    frame_start = int(action.frame_range[0])
    frame_end = int(action.frame_range[1])
    if (
        export_settings.get("gltf_negative_frames") == "CROP"
        and frame_start < 0
    ):
        frame_start = 0
    if export_settings.get("gltf_frame_range"):
        frame_start = max(bpy.context.scene.frame_start, frame_start)
        frame_end = min(bpy.context.scene.frame_end, frame_end)
    return frame_start, frame_end, target_ids


def _range_candidates(action_name, target_ids, export_settings):
    ranges = export_settings.get("ranges", {})
    keys = target_ids if target_ids else ranges.keys()
    candidates = []
    for target_id in keys:
        action_range = ranges.get(target_id, {}).get(action_name)
        if action_range is None:
            continue
        candidates.append(
            (
                target_id,
                float(action_range["start"]),
                float(action_range["end"]),
            )
        )
    return candidates


def _target_ids(blender_object, export_settings):
    tree = export_settings.get("vtree")
    nodes = getattr(tree, "nodes", {})
    return [
        target_id
        for target_id, node in nodes.items()
        if getattr(node, "blender_object", None) == blender_object
    ]


def _action_time_origin(action_name, frame_start, target_ids, export_settings):
    slides = export_settings.get("slide", {})
    candidates = {
        float(slides[target_id][action_name])
        for target_id in target_ids
        if target_id in slides and action_name in slides[target_id]
    }
    if not candidates:
        candidates = {
            float(per_target[action_name])
            for per_target in slides.values()
            if action_name in per_target
        }
    if len(candidates) > 1:
        raise MetadataError(
            f"action `{action_name}` is exported with different timestamp "
            "offsets and cannot have one unambiguous marker track"
        )
    if candidates:
        return candidates.pop()

    # Compatibility fallback mirroring the exporter's documented slide modes.
    if export_settings.get("gltf_anim_slide_to_zero"):
        return frame_start
    if (
        export_settings.get("gltf_negative_frames") == "SLIDE"
        and frame_start < 0
    ):
        return frame_start
    return 0.0


_CLASSES = (
    BlackflowerExportSettings,
    BlackflowerAnimationMetadata,
    BlackflowerNodeMetadata,
    BlackflowerMaterialMetadata,
    BLACKFLOWER_PT_animation_metadata,
    BLACKFLOWER_PT_node_metadata,
    BLACKFLOWER_PT_material_metadata,
)


def register():
    for class_type in _CLASSES:
        bpy.utils.register_class(class_type)
    bpy.types.Scene.blackflower_gltf_export = PointerProperty(
        type=BlackflowerExportSettings
    )
    bpy.types.Action.blackflower_animation_metadata = PointerProperty(
        type=BlackflowerAnimationMetadata
    )
    bpy.types.Object.blackflower_node_metadata = PointerProperty(
        type=BlackflowerNodeMetadata
    )
    bpy.types.Material.blackflower_material_metadata = PointerProperty(
        type=BlackflowerMaterialMetadata
    )

    from io_scene_gltf2 import exporter_extension_layout_draw

    exporter_extension_layout_draw[_EXPORT_PANEL_KEY] = draw_export


def unregister():
    from io_scene_gltf2 import exporter_extension_layout_draw

    exporter_extension_layout_draw.pop(_EXPORT_PANEL_KEY, None)
    del bpy.types.Material.blackflower_material_metadata
    del bpy.types.Object.blackflower_node_metadata
    del bpy.types.Action.blackflower_animation_metadata
    del bpy.types.Scene.blackflower_gltf_export
    for class_type in reversed(_CLASSES):
        bpy.utils.unregister_class(class_type)
