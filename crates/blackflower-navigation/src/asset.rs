use std::num::NonZeroU32;

use bytes::{BufMut, Bytes, BytesMut};
use glam::Vec3A;

use crate::{
    Error, MAX_AREAS, NavMesh, NavMeshParams, QueryFilter, detour_navmesh_version,
    recastnavigation_version,
};

const MAGIC: &[u8; 8] = b"BFNAV\0\0\0";
const HASH_BYTES: usize = 32;
const MAX_TEXT_BYTES: usize = 128;
const POLY_REF_BITS: u32 = 32;

/// Current deterministic Blackflower navigation container schema.
pub const NAVIGATION_ASSET_SCHEMA: u32 = 1;

/// Stable identifier for one physical navigation-agent profile.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NavAgentProfileId(String);

impl NavAgentProfileId {
    /// Construct a portable lower-snake-case profile identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if !portable_key(&value, MAX_TEXT_BYTES) {
            return Err(Error::InvalidProfileId);
        }
        Ok(Self(value))
    }

    /// Return the manifest-authored identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Physical dimensions used both while baking and when selecting a navmesh.
#[derive(Debug, Clone, PartialEq)]
pub struct NavAgentProfile {
    id: NavAgentProfileId,
    height: f32,
    radius: f32,
    max_climb: f32,
    max_slope_degrees: f32,
}

impl NavAgentProfile {
    /// Construct a fully explicit physical agent profile.
    pub fn new(
        id: NavAgentProfileId,
        height: f32,
        radius: f32,
        max_climb: f32,
        max_slope_degrees: f32,
    ) -> Result<Self, Error> {
        if !positive(height)
            || !positive(radius)
            || !non_negative(max_climb)
            || !max_slope_degrees.is_finite()
            || !(0.0..90.0).contains(&max_slope_degrees)
        {
            return Err(Error::InvalidAgentProfile);
        }
        Ok(Self {
            id,
            height,
            radius,
            max_climb,
            max_slope_degrees,
        })
    }

    /// Stable profile identifier.
    #[must_use]
    pub const fn id(&self) -> &NavAgentProfileId {
        &self.id
    }

    /// Agent height in world units.
    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height
    }

    /// Agent radius in world units.
    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    /// Maximum climb height in world units.
    #[must_use]
    pub const fn max_climb(&self) -> f32 {
        self.max_climb
    }

    /// Maximum traversable surface slope.
    #[must_use]
    pub const fn max_slope_degrees(&self) -> f32 {
        self.max_slope_degrees
    }

    /// Canonical BLAKE3 identity of the physical dimensions.
    #[must_use]
    pub fn physical_hash(&self) -> [u8; HASH_BYTES] {
        hash_agent(self)
    }
}

/// Explicit Recast settings retained in every cooked navigation asset.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationBuildSettings {
    cell_size: f32,
    cell_height: f32,
    tile_size: u32,
    region_min_area: u32,
    region_merge_area: u32,
    max_edge_length: f32,
    max_simplification_error: f32,
    max_vertices_per_polygon: u32,
    detail_sample_distance: f32,
    detail_sample_max_error: f32,
}

impl NavigationBuildSettings {
    /// Construct a complete, inheritance-free Recast build configuration.
    #[allow(
        clippy::too_many_arguments,
        reason = "the asset format intentionally requires every Recast setting explicitly"
    )]
    pub fn new(
        cell_size: f32,
        cell_height: f32,
        tile_size: u32,
        region_min_area: u32,
        region_merge_area: u32,
        max_edge_length: f32,
        max_simplification_error: f32,
        max_vertices_per_polygon: u32,
        detail_sample_distance: f32,
        detail_sample_max_error: f32,
    ) -> Result<Self, Error> {
        if !positive(cell_size)
            || !positive(cell_height)
            || tile_size == 0
            || tile_size > i32::MAX.cast_unsigned()
            || region_min_area == 0
            || region_min_area > 46_340
            || region_merge_area == 0
            || region_merge_area > 46_340
            || !positive(max_edge_length)
            || !positive(max_simplification_error)
            || !(3..=6).contains(&max_vertices_per_polygon)
            || !non_negative(detail_sample_distance)
            || !non_negative(detail_sample_max_error)
        {
            return Err(Error::InvalidBuildSettings);
        }
        Ok(Self {
            cell_size,
            cell_height,
            tile_size,
            region_min_area,
            region_merge_area,
            max_edge_length,
            max_simplification_error,
            max_vertices_per_polygon,
            detail_sample_distance,
            detail_sample_max_error,
        })
    }

    /// Horizontal voxel size.
    #[must_use]
    pub const fn cell_size(&self) -> f32 {
        self.cell_size
    }

    /// Vertical voxel size.
    #[must_use]
    pub const fn cell_height(&self) -> f32 {
        self.cell_height
    }

    /// Tile edge in cells, excluding Recast's generated border.
    #[must_use]
    pub const fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Minimum region linear size; Recast receives its square in cells.
    #[must_use]
    pub const fn region_min_area(&self) -> u32 {
        self.region_min_area
    }

    /// Merge-region linear size; Recast receives its square in cells.
    #[must_use]
    pub const fn region_merge_area(&self) -> u32 {
        self.region_merge_area
    }

    /// Maximum contour edge length in world units.
    #[must_use]
    pub const fn max_edge_length(&self) -> f32 {
        self.max_edge_length
    }

    /// Maximum contour simplification error.
    #[must_use]
    pub const fn max_simplification_error(&self) -> f32 {
        self.max_simplification_error
    }

    /// Maximum vertices in one Detour polygon.
    #[must_use]
    pub const fn max_vertices_per_polygon(&self) -> u32 {
        self.max_vertices_per_polygon
    }

    /// Detail-mesh sampling distance multiplier.
    #[must_use]
    pub const fn detail_sample_distance(&self) -> f32 {
        self.detail_sample_distance
    }

    /// Detail-mesh maximum sample error multiplier.
    #[must_use]
    pub const fn detail_sample_max_error(&self) -> f32 {
        self.detail_sample_max_error
    }

    /// Canonical BLAKE3 identity of all Recast settings.
    #[must_use]
    pub fn settings_hash(&self) -> [u8; HASH_BYTES] {
        hash_build(self)
    }
}

/// Stable semantic name mapped to a Detour area identifier by the cooker.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NavigationAreaKey(String);

impl NavigationAreaKey {
    /// Construct a portable lower-snake-case semantic key.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if !portable_key(&value, 64) {
            return Err(Error::InvalidAreaKey);
        }
        Ok(Self(value))
    }

    /// Return the authored semantic key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One manifest-authored area and its baked native query policy.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationArea {
    id: u8,
    key: NavigationAreaKey,
    traversable: bool,
    cost: f32,
}

impl NavigationArea {
    /// Construct one canonical area entry.
    ///
    /// Traversable areas require a finite positive cost. Blocked areas require
    /// no cost and are encoded with the canonical zero value.
    pub fn new(
        id: u8,
        key: NavigationAreaKey,
        traversable: bool,
        cost: Option<f32>,
    ) -> Result<Self, Error> {
        if usize::from(id) >= MAX_AREAS {
            return Err(Error::InvalidArea(id));
        }
        let cost = if traversable {
            let cost = cost.ok_or(Error::InvalidAreaCost)?;
            if !positive(cost) {
                return Err(Error::InvalidAreaCost);
            }
            cost
        } else {
            if cost.is_some_and(|value| value != 0.0) {
                return Err(Error::InvalidAreaCost);
            }
            0.0
        };
        Ok(Self {
            id,
            key,
            traversable,
            cost,
        })
    }

    /// Detour area identifier assigned by canonical key order.
    #[must_use]
    pub const fn id(&self) -> u8 {
        self.id
    }

    /// Authored semantic area key.
    #[must_use]
    pub const fn key(&self) -> &NavigationAreaKey {
        &self.key
    }

    /// Whether polygons in this area receive the traversable polygon flag.
    #[must_use]
    pub const fn traversable(&self) -> bool {
        self.traversable
    }

    /// Native Detour traversal multiplier, or zero for a blocked area.
    #[must_use]
    pub const fn cost(&self) -> f32 {
        self.cost
    }
}

/// One ordered Detour tile payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationTile {
    x: i32,
    y: i32,
    layer: i32,
    data: Bytes,
}

impl NavigationTile {
    /// Construct one non-empty tile.
    pub fn new(x: i32, y: i32, layer: i32, data: Bytes) -> Result<Self, Error> {
        if data.is_empty() {
            return Err(invalid("navigation tile payload is empty"));
        }
        Ok(Self { x, y, layer, data })
    }

    /// Recast tile-grid coordinate.
    #[must_use]
    pub const fn coordinate(&self) -> (i32, i32, i32) {
        (self.x, self.y, self.layer)
    }

    /// Detour tile bytes.
    #[must_use]
    pub const fn data(&self) -> &Bytes {
        &self.data
    }
}

/// Validated `.bfnav` bytes and the runtime metadata needed to instantiate it.
#[derive(Debug, Clone)]
pub struct NavMeshAsset {
    bytes: Bytes,
    agent: NavAgentProfile,
    build: NavigationBuildSettings,
    params: NavMeshParams,
    areas: Vec<NavigationArea>,
    tiles: Vec<NavigationTile>,
}

impl NavMeshAsset {
    /// Build and canonically encode a cooked navigation asset.
    pub fn new(
        agent: NavAgentProfile,
        build: NavigationBuildSettings,
        params: NavMeshParams,
        areas: Vec<NavigationArea>,
        tiles: Vec<NavigationTile>,
    ) -> Result<Self, Error> {
        validate_collections(&areas, &tiles, params)?;
        let bytes = encode(&agent, &build, params, &areas, &tiles)?;
        Ok(Self {
            bytes,
            agent,
            build,
            params,
            areas,
            tiles,
        })
    }

    /// Decode and completely validate authenticated `.bfnav` bytes.
    pub fn from_bytes(bytes: Bytes) -> Result<Self, Error> {
        let decoded = decode(&bytes)?;
        Ok(Self {
            bytes,
            agent: decoded.agent,
            build: decoded.build,
            params: decoded.params,
            areas: decoded.areas,
            tiles: decoded.tiles,
        })
    }

    /// Original canonical container bytes.
    #[must_use]
    pub const fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Physical agent baked into this mesh.
    #[must_use]
    pub const fn agent(&self) -> &NavAgentProfile {
        &self.agent
    }

    /// Complete Recast build configuration.
    #[must_use]
    pub const fn build_settings(&self) -> &NavigationBuildSettings {
        &self.build
    }

    /// Canonically ordered semantic area table.
    #[must_use]
    pub fn areas(&self) -> &[NavigationArea] {
        &self.areas
    }

    /// Canonically ordered Detour tiles.
    #[must_use]
    pub fn tiles(&self) -> &[NavigationTile] {
        &self.tiles
    }

    /// Instantiate the Detour navigation mesh and copy all cooked tiles.
    pub fn instantiate(&self) -> Result<NavMesh, Error> {
        let mut navmesh = NavMesh::tiled(self.params)?;
        for tile in &self.tiles {
            let _reference = navmesh.add_tile(&tile.data)?;
        }
        Ok(navmesh)
    }

    /// Compile the baked area table into Detour's native filter arrays.
    pub fn query_filter(&self) -> Result<QueryFilter, Error> {
        let mut filter = QueryFilter::new()
            .with_include_flags(1)
            .with_exclude_flags(0);
        for area in &self.areas {
            if area.traversable {
                filter = filter.with_area_cost(area.id, area.cost)?;
            }
        }
        Ok(filter)
    }
}

struct Decoded {
    agent: NavAgentProfile,
    build: NavigationBuildSettings,
    params: NavMeshParams,
    areas: Vec<NavigationArea>,
    tiles: Vec<NavigationTile>,
}

#[allow(
    clippy::too_many_lines,
    reason = "the linear decoder mirrors the compact versioned binary layout"
)]
fn decode(bytes: &[u8]) -> Result<Decoded, Error> {
    let mut reader = Reader::new(bytes);
    if reader.take(MAGIC.len())? != MAGIC {
        return Err(invalid("invalid navigation asset magic"));
    }
    let schema = reader.u32()?;
    if schema != NAVIGATION_ASSET_SCHEMA {
        return Err(Error::UnsupportedAssetSchema(schema));
    }
    let recast = (reader.u32()?, reader.u32()?, reader.u32()?);
    let detour = reader.u32()?;
    let poly_ref_bits = reader.u32()?;
    if recast != recastnavigation_version()
        || detour != detour_navmesh_version()
        || poly_ref_bits != POLY_REF_BITS
    {
        return Err(Error::IncompatibleAssetToolchain);
    }
    let id = NavAgentProfileId::new(reader.text(MAX_TEXT_BYTES)?)?;
    let agent = NavAgentProfile::new(
        id,
        reader.f32()?,
        reader.f32()?,
        reader.f32()?,
        reader.f32()?,
    )?;
    let agent_hash = reader.array::<HASH_BYTES>()?;
    if agent_hash != agent.physical_hash() {
        return Err(invalid("navigation agent hash does not match its values"));
    }
    let build = NavigationBuildSettings::new(
        reader.f32()?,
        reader.f32()?,
        reader.u32()?,
        reader.u32()?,
        reader.u32()?,
        reader.f32()?,
        reader.f32()?,
        reader.u32()?,
        reader.f32()?,
        reader.f32()?,
    )?;
    let build_hash = reader.array::<HASH_BYTES>()?;
    if build_hash != build.settings_hash() {
        return Err(invalid("navigation build hash does not match its values"));
    }
    let params = NavMeshParams::new(
        Vec3A::new(reader.f32()?, reader.f32()?, reader.f32()?),
        reader.f32()?,
        reader.f32()?,
        NonZeroU32::new(reader.u32()?).ok_or(Error::InvalidNavMeshParameters)?,
        NonZeroU32::new(reader.u32()?).ok_or(Error::InvalidNavMeshParameters)?,
    )?;
    let area_count = reader.len(MAX_AREAS, "area")?;
    let mut areas = Vec::with_capacity(area_count);
    for _ in 0..area_count {
        let id = reader.u8()?;
        let traversable = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(invalid("invalid navigation area traversal flag")),
        };
        if reader.u16()? != 0 {
            return Err(invalid("navigation area reserved bits are non-zero"));
        }
        let cost = reader.f32()?;
        let key = NavigationAreaKey::new(reader.text(64)?)?;
        areas.push(NavigationArea::new(id, key, traversable, Some(cost))?);
    }
    let tile_count = reader.len(
        usize::try_from(params.max_tiles().get()).unwrap_or(usize::MAX),
        "tile",
    )?;
    let mut tiles = Vec::with_capacity(tile_count);
    for _ in 0..tile_count {
        let coordinate = (reader.i32()?, reader.i32()?, reader.i32()?);
        let length = reader.len(reader.remaining(), "tile byte")?;
        let data = Bytes::copy_from_slice(reader.take(length)?);
        tiles.push(NavigationTile::new(
            coordinate.0,
            coordinate.1,
            coordinate.2,
            data,
        )?);
    }
    if reader.remaining() != 0 {
        return Err(invalid("navigation asset has trailing bytes"));
    }
    validate_collections(&areas, &tiles, params)?;
    Ok(Decoded {
        agent,
        build,
        params,
        areas,
        tiles,
    })
}

fn encode(
    agent: &NavAgentProfile,
    build: &NavigationBuildSettings,
    params: NavMeshParams,
    areas: &[NavigationArea],
    tiles: &[NavigationTile],
) -> Result<Bytes, Error> {
    let mut output = BytesMut::new();
    output.extend_from_slice(MAGIC);
    output.put_u32_le(NAVIGATION_ASSET_SCHEMA);
    let recast = recastnavigation_version();
    output.put_u32_le(recast.0);
    output.put_u32_le(recast.1);
    output.put_u32_le(recast.2);
    output.put_u32_le(detour_navmesh_version());
    output.put_u32_le(POLY_REF_BITS);
    put_text(&mut output, agent.id.as_str())?;
    put_agent(&mut output, agent);
    output.extend_from_slice(&agent.physical_hash());
    put_build(&mut output, build);
    output.extend_from_slice(&build.settings_hash());
    let origin = params.origin();
    put_f32(&mut output, origin.x);
    put_f32(&mut output, origin.y);
    put_f32(&mut output, origin.z);
    put_f32(&mut output, params.tile_width());
    put_f32(&mut output, params.tile_height());
    output.put_u32_le(params.max_tiles().get());
    output.put_u32_le(params.max_polygons_per_tile().get());
    put_len(&mut output, areas.len(), "area")?;
    for area in areas {
        output.put_u8(area.id);
        output.put_u8(u8::from(area.traversable));
        output.put_u16_le(0);
        put_f32(&mut output, area.cost);
        put_text(&mut output, area.key.as_str())?;
    }
    put_len(&mut output, tiles.len(), "tile")?;
    for tile in tiles {
        output.put_i32_le(tile.x);
        output.put_i32_le(tile.y);
        output.put_i32_le(tile.layer);
        put_len(&mut output, tile.data.len(), "tile byte")?;
        output.extend_from_slice(&tile.data);
    }
    Ok(output.freeze())
}

fn validate_collections(
    areas: &[NavigationArea],
    tiles: &[NavigationTile],
    params: NavMeshParams,
) -> Result<(), Error> {
    if areas.is_empty() || areas.len() > MAX_AREAS {
        return Err(invalid(
            "navigation asset must declare from 1 through 64 areas",
        ));
    }
    for (index, area) in areas.iter().enumerate() {
        if usize::from(area.id) != index {
            return Err(invalid("navigation area identifiers must be contiguous"));
        }
    }
    if areas
        .windows(2)
        .any(|pair| pair[0].key.as_str() >= pair[1].key.as_str())
    {
        return Err(invalid("navigation areas must be sorted by unique key"));
    }
    if tiles.is_empty()
        || u32::try_from(tiles.len()).map_or(true, |count| count > params.max_tiles().get())
    {
        return Err(invalid("navigation tile count is outside navmesh capacity"));
    }
    if tiles
        .windows(2)
        .any(|pair| pair[0].coordinate() >= pair[1].coordinate())
    {
        return Err(invalid(
            "navigation tiles must be strictly ordered by coordinate",
        ));
    }
    Ok(())
}

fn put_agent(output: &mut BytesMut, agent: &NavAgentProfile) {
    put_f32(output, agent.height);
    put_f32(output, agent.radius);
    put_f32(output, agent.max_climb);
    put_f32(output, agent.max_slope_degrees);
}

fn put_build(output: &mut BytesMut, build: &NavigationBuildSettings) {
    put_f32(output, build.cell_size);
    put_f32(output, build.cell_height);
    output.put_u32_le(build.tile_size);
    output.put_u32_le(build.region_min_area);
    output.put_u32_le(build.region_merge_area);
    put_f32(output, build.max_edge_length);
    put_f32(output, build.max_simplification_error);
    output.put_u32_le(build.max_vertices_per_polygon);
    put_f32(output, build.detail_sample_distance);
    put_f32(output, build.detail_sample_max_error);
}

fn hash_agent(agent: &NavAgentProfile) -> [u8; HASH_BYTES] {
    let mut bytes = BytesMut::new();
    put_agent(&mut bytes, agent);
    *blake3::hash(&bytes).as_bytes()
}

fn hash_build(build: &NavigationBuildSettings) -> [u8; HASH_BYTES] {
    let mut bytes = BytesMut::new();
    put_build(&mut bytes, build);
    *blake3::hash(&bytes).as_bytes()
}

fn put_f32(output: &mut BytesMut, value: f32) {
    output.put_u32_le(value.to_bits());
}

fn put_len(output: &mut BytesMut, length: usize, label: &str) -> Result<(), Error> {
    let length = u32::try_from(length)
        .map_err(|_error| invalid(format!("{label} count exceeds the format limit")))?;
    output.put_u32_le(length);
    Ok(())
}

fn put_text(output: &mut BytesMut, value: &str) -> Result<(), Error> {
    put_len(output, value.len(), "text byte")?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn non_negative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}

fn portable_key(value: &str, maximum: usize) -> bool {
    value.len() <= maximum
        && value
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidAsset(message.into())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid("navigation asset offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| invalid("truncated navigation asset"))?;
        self.offset = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.take(N)?
            .try_into()
            .map_err(|_error| invalid("truncated navigation asset field"))
    }

    fn u8(&mut self) -> Result<u8, Error> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| invalid("truncated navigation asset byte"))
    }

    fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32, Error> {
        let value = f32::from_bits(self.u32()?);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(invalid("navigation asset contains a non-finite float"))
        }
    }

    fn len(&mut self, maximum: usize, label: &str) -> Result<usize, Error> {
        let value = usize::try_from(self.u32()?)
            .map_err(|_error| invalid(format!("{label} count does not fit this platform")))?;
        if value > maximum {
            return Err(invalid(format!("{label} count exceeds its limit")));
        }
        Ok(value)
    }

    fn text(&mut self, maximum: usize) -> Result<String, Error> {
        let length = self.len(maximum, "text byte")?;
        std::str::from_utf8(self.take(length)?)
            .map(str::to_owned)
            .map_err(|_error| invalid("navigation asset text is not UTF-8"))
    }
}

#[cfg(test)]
#[path = "../tests/unit/asset.rs"]
mod tests;
