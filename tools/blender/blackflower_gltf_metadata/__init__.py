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
    build_map_material_metadata,
    build_map_node_metadata,
    merge_extras,
    validate_map_references,
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
    """Schema-1 map metadata attached to an exported Blender object."""

    enabled: BoolProperty(
        name="Blackflower Map Node",
        description="Attach typed Blackflower map metadata to this glTF node",
        default=False,
    )
    identifier: StringProperty(
        name="ID",
        description="Required stable lower_snake_case identifier within the map",
        default="",
        maxlen=128,
    )
    role: EnumProperty(
        name="Role",
        description="The node's single primary map role",
        items=(
            ("geometry", "Geometry", "Map geometry with combined domain uses"),
            ("spawn_point", "Spawn Point", "Named spawn transform"),
            ("prefab_instance", "Prefab Instance", "Instance a prefab asset"),
            ("volume_instance", "Volume Instance", "Instance a volume asset"),
            ("trigger_volume", "Trigger Volume", "Bound a trigger definition"),
            ("navigation_anchor", "Navigation Anchor", "Navigation endpoint"),
            ("navigation_link", "Navigation Link", "Connection to an anchor"),
            ("acoustic_zone", "Acoustic Zone", "Zone identity, bounds, or probes"),
            ("acoustic_portal", "Acoustic Portal", "Portal between zone bounds"),
            ("audio_emitter", "Audio Emitter", "Placed sound source"),
        ),
        default="geometry",
    )
    render: BoolProperty(
        name="Render",
        description="Include geometry in the presentation projection",
        default=True,
    )
    collision: BoolProperty(
        name="Collision",
        description="Include geometry in the simulation collision projection",
        default=False,
    )
    navigation: EnumProperty(
        name="Navigation",
        description="How geometry participates in navigation cooking",
        items=(
            ("none", "None", "Exclude from navigation cooking"),
            ("surface", "Surface", "Rasterize as walkable input"),
            ("obstacle", "Obstacle", "Rasterize as blocked input"),
        ),
        default="none",
    )
    acoustic_class: EnumProperty(
        name="Acoustic Class",
        description="How geometry participates in acoustic cooking",
        items=(
            ("ignored", "Ignored", "Exclude from acoustic cooking"),
            ("static", "Static", "Include in the static acoustic scene"),
            (
                "dynamic_rigid",
                "Dynamic Rigid",
                "Select through acoustic prefabs",
            ),
            (
                "dynamic_state",
                "Dynamic State",
                "Select through acoustic prefab variants",
            ),
        ),
        default="ignored",
    )
    spawn_set: StringProperty(
        name="Set",
        description="Stable spawn set key",
        default="default",
        maxlen=64,
    )
    spawn_weight: FloatProperty(
        name="Weight",
        description="Positive relative selection weight",
        default=1.0,
        min=0.0001,
    )
    asset: StringProperty(
        name="Asset",
        description="Portable prefab or volume asset ID",
        default="",
        maxlen=255,
    )
    trigger_definition: StringProperty(
        name="Definition",
        description="Portable trigger definition asset ID",
        default="",
        maxlen=255,
    )
    navigation_end: PointerProperty(
        name="End Anchor",
        description="Navigation Anchor at the end of this link",
        type=bpy.types.Object,
    )
    navigation_area: StringProperty(
        name="Area",
        description="Area key declared by the navigation asset",
        default="",
        maxlen=64,
    )
    navigation_direction: EnumProperty(
        name="Direction",
        items=(
            ("bidirectional", "Bidirectional", "Allow travel both ways"),
            ("one_way", "One Way", "Travel from this node to the end anchor"),
        ),
        default="bidirectional",
    )
    navigation_radius: FloatProperty(
        name="Radius",
        description="Positive endpoint matching radius in world units",
        default=0.5,
        min=0.0001,
    )
    acoustic_zone_kind: EnumProperty(
        name="Zone Kind",
        description="Whether this node names, bounds, or seeds probes for a zone",
        items=(
            ("identity", "Identity", "Declare a stable zone identity"),
            ("bounds", "Bounds", "Bound an acoustic zone"),
            ("probes", "Probes", "Bound probe generation for a zone identity"),
        ),
        default="bounds",
    )
    acoustic_zone: PointerProperty(
        name="Zone",
        description="Acoustic Zone identity used by these probe bounds",
        type=bpy.types.Object,
    )
    acoustic_zone_a: PointerProperty(
        name="Zone A",
        description="First adjacent Acoustic Zone bounds object",
        type=bpy.types.Object,
    )
    acoustic_zone_b: PointerProperty(
        name="Zone B",
        description="Second adjacent Acoustic Zone bounds object",
        type=bpy.types.Object,
    )
    acoustic_controller: PointerProperty(
        name="Controller",
        description="Optional Geometry or Prefab Instance controlling the portal",
        type=bpy.types.Object,
    )
    acoustic_initially_open: BoolProperty(
        name="Initially Open",
        description="Initial authoritative portal state",
        default=True,
    )
    sound: StringProperty(
        name="Sound",
        description="Portable audio asset ID",
        default="",
        maxlen=255,
    )
    autoplay: BoolProperty(
        name="Autoplay",
        description="Start this emitter when the map becomes active",
        default=False,
    )


class BlackflowerMaterialMetadata(PropertyGroup):
    """Typed surface metadata attached to a Blender material."""

    physics_material: StringProperty(
        name="Physics Material",
        description="Portable physics material asset ID",
        default="",
        maxlen=255,
    )
    navigation_area: StringProperty(
        name="Navigation Area",
        description="Portable navigation area key",
        default="",
        maxlen=64,
    )

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
    """Object Properties panel for typed map node metadata."""

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
        fields = layout.column()
        fields.enabled = properties.enabled
        fields.prop(properties, "identifier")
        fields.prop(properties, "role")
        if properties.enabled and not properties.identifier:
            fields.label(text="Stable ID is required for export", icon="ERROR")

        role = properties.role
        if role == "geometry":
            fields.prop(properties, "render")
            fields.prop(properties, "collision")
            fields.prop(properties, "navigation")
            fields.prop(properties, "acoustic_class")
        elif role == "spawn_point":
            fields.prop(properties, "spawn_set")
            fields.prop(properties, "spawn_weight")
        elif role in {"prefab_instance", "volume_instance"}:
            fields.prop(properties, "asset")
        elif role == "trigger_volume":
            fields.prop(properties, "trigger_definition")
        elif role == "navigation_link":
            fields.prop(properties, "navigation_end")
            fields.prop(properties, "navigation_area")
            fields.prop(properties, "navigation_direction")
            fields.prop(properties, "navigation_radius")
        elif role == "acoustic_zone":
            fields.prop(properties, "acoustic_zone_kind")
            if properties.acoustic_zone_kind == "probes":
                fields.prop(properties, "acoustic_zone")
        elif role == "acoustic_portal":
            fields.prop(properties, "acoustic_zone_a")
            fields.prop(properties, "acoustic_zone_b")
            fields.prop(properties, "acoustic_controller")
            fields.prop(properties, "acoustic_initially_open")
        elif role == "audio_emitter":
            fields.prop(properties, "sound")
            fields.prop(properties, "autoplay")


class BLACKFLOWER_PT_material_metadata(Panel):
    """Material Properties panel for typed map surface metadata."""

    bl_label = "Blackflower Surface"
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
        properties = context.material.blackflower_material_metadata
        layout.prop(properties, "physics_material")
        layout.prop(properties, "navigation_area")
        layout.prop(properties, "acoustic_material")


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
        body.label(text="Object metadata becomes typed map node extras.")
        body.label(text="Material mappings become typed map surface extras.")


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
        """Attach typed schema-1 map metadata to one glTF node."""

        del export_settings
        if not self.properties.enabled or not isinstance(
            blender_object, bpy.types.Object
        ):
            return

        properties = blender_object.blackflower_node_metadata
        if not properties.enabled:
            return

        role = properties.role
        _validate_object_role(blender_object, role, properties)
        metadata = build_map_node_metadata(
            role,
            properties.identifier,
            render=getattr(properties, "render", False),
            collision=getattr(properties, "collision", False),
            navigation=getattr(properties, "navigation", "none"),
            acoustic_class=getattr(properties, "acoustic_class", "ignored"),
            spawn_set=getattr(properties, "spawn_set", "default"),
            spawn_weight=getattr(properties, "spawn_weight", 1.0),
            asset=getattr(properties, "asset", ""),
            definition=getattr(properties, "trigger_definition", ""),
            navigation_end=_referenced_identifier(
                getattr(properties, "navigation_end", None),
                {"navigation_anchor"},
                "navigation link end",
            ),
            navigation_area=getattr(properties, "navigation_area", ""),
            navigation_direction=getattr(
                properties, "navigation_direction", "bidirectional"
            ),
            navigation_radius=getattr(properties, "navigation_radius", 0.5),
            acoustic_zone_kind=getattr(
                properties, "acoustic_zone_kind", "bounds"
            ),
            acoustic_zone=_referenced_identifier(
                getattr(properties, "acoustic_zone", None),
                {"acoustic_zone"},
                "probe zone",
            ),
            acoustic_zone_a=_referenced_identifier(
                getattr(properties, "acoustic_zone_a", None),
                {"acoustic_zone"},
                "portal zone A",
            ),
            acoustic_zone_b=_referenced_identifier(
                getattr(properties, "acoustic_zone_b", None),
                {"acoustic_zone"},
                "portal zone B",
            ),
            acoustic_controller=_referenced_identifier(
                getattr(properties, "acoustic_controller", None),
                {"geometry", "prefab_instance"},
                "portal controller",
            ),
            acoustic_initially_open=getattr(
                properties, "acoustic_initially_open", True
            ),
            sound=getattr(properties, "sound", ""),
            autoplay=getattr(properties, "autoplay", False),
        )
        gltf2_node.extras = merge_extras(gltf2_node.extras, metadata)

    def gather_gltf_hook(
        self,
        active_scene_index,
        scenes,
        animations,
        export_settings,
    ):
        """Validate map-wide IDs and references after all nodes are gathered."""

        del active_scene_index, animations, export_settings
        if self.properties.enabled:
            for scene in scenes:
                validate_map_references(_map_metadata_in_scene(scene))

    def gather_material_hook(
        self,
        gltf2_material,
        blender_material,
        export_settings,
    ):
        """Attach typed schema-1 surface metadata."""

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
        metadata = build_map_material_metadata(
            physics_material=getattr(properties, "physics_material", ""),
            navigation_area=getattr(properties, "navigation_area", ""),
            acoustic_material=getattr(properties, "acoustic_material", ""),
        )
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


def _referenced_identifier(target, expected_roles, label):
    if target is None:
        return ""
    properties = getattr(target, "blackflower_node_metadata", None)
    if properties is None or not getattr(properties, "enabled", False):
        raise MetadataError(f"{label} must reference an enabled Blackflower map node")
    role = getattr(properties, "role", "")
    if role not in expected_roles:
        choices = ", ".join(sorted(expected_roles))
        raise MetadataError(f"{label} must reference role {choices}, not `{role}`")
    identifier = getattr(properties, "identifier", "")
    if not identifier:
        raise MetadataError(f"{label} references a map node without a stable ID")
    return identifier


def _validate_object_role(blender_object, role, properties):
    object_type = getattr(blender_object, "type", None)
    if object_type is None:
        return
    mesh_roles = {"geometry", "trigger_volume", "acoustic_portal"}
    if role == "acoustic_zone":
        expected = "EMPTY" if properties.acoustic_zone_kind == "identity" else "MESH"
    else:
        expected = "MESH" if role in mesh_roles else "EMPTY"
    if object_type != expected:
        raise MetadataError(
            f"map role `{role}` requires a Blender {expected.title()} object"
        )


def _map_metadata_in_scene(scene):
    metadata = []
    pending = list(getattr(scene, "nodes", None) or [])
    visited = set()
    while pending:
        node = pending.pop()
        identity = id(node)
        if identity in visited:
            continue
        visited.add(identity)
        blackflower = _blackflower_extra(node)
        if isinstance(blackflower, dict) and "node" in blackflower:
            metadata.append(blackflower)
        pending.extend(getattr(node, "children", None) or [])
    return metadata


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
