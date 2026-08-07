use std::cell::RefCell;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::rc::Rc;

use glam::Vec3A;

use crate::error::Error;
use crate::ffi::{self, Details, Status};
use crate::filter::QueryFilter;
use crate::navmesh::{NavMesh, PolygonRef};

const DEFAULT_MAX_NODES: u32 = 2_048;
const DEFAULT_MAX_PATH_POLYGONS: u32 = 256;
const DEFAULT_MAX_STRAIGHT_PATH_POINTS: u32 = 256;

/// Configuration used to create a [`Query`].
pub struct QueryBuilder<'navmesh> {
    navmesh: &'navmesh NavMesh,
    max_nodes: NonZeroU32,
    max_path_polygons: NonZeroU32,
    max_straight_path_points: NonZeroU32,
}

impl<'navmesh> QueryBuilder<'navmesh> {
    pub(crate) const fn new(navmesh: &'navmesh NavMesh) -> Self {
        Self {
            navmesh,
            max_nodes: nonzero(DEFAULT_MAX_NODES),
            max_path_polygons: nonzero(DEFAULT_MAX_PATH_POLYGONS),
            max_straight_path_points: nonzero(DEFAULT_MAX_STRAIGHT_PATH_POINTS),
        }
    }

    /// Set Detour's A* search-node capacity.
    #[must_use]
    pub const fn max_nodes(mut self, max_nodes: NonZeroU32) -> Self {
        self.max_nodes = max_nodes;
        self
    }

    /// Set the maximum polygon corridor returned by pathfinding and raycasts.
    #[must_use]
    pub const fn max_path_polygons(mut self, max_path_polygons: NonZeroU32) -> Self {
        self.max_path_polygons = max_path_polygons;
        self
    }

    /// Set the maximum number of points returned by straight-path extraction.
    #[must_use]
    pub const fn max_straight_path_points(mut self, max_straight_path_points: NonZeroU32) -> Self {
        self.max_straight_path_points = max_straight_path_points;
        self
    }

    /// Allocate and initialize one Detour query object.
    pub fn build(self) -> Result<Query<'navmesh>, Error> {
        if self.max_nodes.get() > 65_535 {
            return Err(Error::QueryNodeCapacityTooLarge(self.max_nodes.get()));
        }
        let max_path_polygons = result_capacity(self.max_path_polygons)?;
        let max_straight_path_points = result_capacity(self.max_straight_path_points)?;
        let pointer = ffi::create_query(self.navmesh.pointer, self.max_nodes.get())
            .map_err(map_query_initialization)?;
        let scratch = QueryScratch::new(max_path_polygons, max_straight_path_points);
        Ok(Query {
            pointer,
            navmesh: self.navmesh,
            scratch: RefCell::new(scratch),
            not_send_sync: PhantomData,
        })
    }
}

/// A point projected onto a Detour navigation polygon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearestPoint {
    polygon: PolygonRef,
    position: Vec3A,
    is_over_polygon: bool,
}

impl NearestPoint {
    /// The polygon containing the projected point.
    #[must_use]
    pub const fn polygon(self) -> PolygonRef {
        self.polygon
    }

    /// The nearest world-space position on the polygon.
    #[must_use]
    pub const fn position(self) -> Vec3A {
        self.position
    }

    /// Whether the source point's horizontal coordinates were over the polygon.
    #[must_use]
    pub const fn is_over_polygon(self) -> bool {
        self.is_over_polygon
    }
}

/// The role of a point returned by Detour's straight-path extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPointKind {
    /// First point of the path.
    Start,
    /// An intermediate turn in the path.
    Corner,
    /// An off-mesh connection endpoint.
    OffMeshConnection,
    /// Final point of the path.
    End,
}

/// One point in a straight path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    position: Vec3A,
    polygon: Option<PolygonRef>,
    kind: PathPointKind,
}

impl PathPoint {
    /// The world-space path position.
    #[must_use]
    pub const fn position(self) -> Vec3A {
        self.position
    }

    /// The polygon entered at this point, if Detour supplied one.
    #[must_use]
    pub const fn polygon(self) -> Option<PolygonRef> {
        self.polygon
    }

    /// The semantic role assigned by Detour.
    #[must_use]
    pub const fn kind(self) -> PathPointKind {
        self.kind
    }
}

/// A Detour polygon corridor and its straightened world-space path.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    corridor: Vec<PolygonRef>,
    points: Vec<PathPoint>,
    partial: bool,
}

impl Path {
    /// The ordered polygons traversed by the query.
    #[must_use]
    pub fn corridor(&self) -> &[PolygonRef] {
        &self.corridor
    }

    /// The straightened path points.
    #[must_use]
    pub fn points(&self) -> &[PathPoint] {
        &self.points
    }

    /// Whether Detour returned the best reachable corridor instead of the end.
    #[must_use]
    pub const fn is_partial(&self) -> bool {
        self.partial
    }
}

/// The result of a short horizontal visibility raycast across the navmesh.
#[derive(Debug, Clone, PartialEq)]
pub struct RaycastHit {
    hit: bool,
    fraction: f32,
    position: Vec3A,
    normal: Vec3A,
    edge_index: Option<u32>,
    path_cost: f32,
    visited: Vec<PolygonRef>,
}

impl RaycastHit {
    /// Whether the ray reached a polygon wall before its target.
    #[must_use]
    pub const fn hit(&self) -> bool {
        self.hit
    }

    /// The segment fraction at which the wall was hit, or one when unobstructed.
    #[must_use]
    pub const fn fraction(&self) -> f32 {
        self.fraction
    }

    /// The interpolated world-space hit or end position.
    #[must_use]
    pub const fn position(&self) -> Vec3A {
        self.position
    }

    /// The wall normal, or zero when the ray was unobstructed.
    #[must_use]
    pub const fn normal(&self) -> Vec3A {
        self.normal
    }

    /// The final polygon edge index when a wall was hit.
    #[must_use]
    pub const fn edge_index(&self) -> Option<u32> {
        self.edge_index
    }

    /// The filter-weighted path cost accumulated by Detour.
    #[must_use]
    pub const fn path_cost(&self) -> f32 {
        self.path_cost
    }

    /// The polygons visited by the raycast.
    #[must_use]
    pub fn visited(&self) -> &[PolygonRef] {
        &self.visited
    }
}

/// A Detour query object borrowing its immutable navigation mesh.
///
/// Queries keep mutable search state internally and are deliberately neither
/// `Send` nor `Sync`. Create one query per runtime worker when queries need to
/// execute in parallel.
pub struct Query<'navmesh> {
    pointer: ffi::QueryPtr,
    navmesh: &'navmesh NavMesh,
    scratch: RefCell<QueryScratch>,
    not_send_sync: PhantomData<Rc<()>>,
}

struct QueryScratch {
    corridor: Vec<u32>,
    straight: StraightPathScratch,
    visited: Vec<u32>,
}

impl QueryScratch {
    fn new(max_path_polygons: usize, max_straight_path_points: usize) -> Self {
        Self {
            corridor: vec![0; max_path_polygons],
            straight: StraightPathScratch {
                positions: vec![ffi::raw::BFNavigationVec3::default(); max_straight_path_points],
                flags: vec![0; max_straight_path_points],
                polygons: vec![0; max_straight_path_points],
            },
            visited: vec![0; max_path_polygons],
        }
    }
}

struct StraightPathScratch {
    positions: Vec<ffi::raw::BFNavigationVec3>,
    flags: Vec<u8>,
    polygons: Vec<u32>,
}

impl Query<'_> {
    /// Find the nearest traversable polygon and projected point.
    pub fn nearest_point(
        &self,
        center: Vec3A,
        half_extents: Vec3A,
        filter: &QueryFilter,
    ) -> Result<Option<NearestPoint>, Error> {
        validate_vector(center)?;
        validate_extents(half_extents)?;
        self.nearest_point_validated(center, half_extents, filter)
    }

    /// Project a position onto a known polygon from this navigation mesh.
    pub fn closest_point(
        &self,
        polygon: PolygonRef,
        position: Vec3A,
    ) -> Result<NearestPoint, Error> {
        self.validate_polygon(polygon)?;
        validate_vector(position)?;
        let nearest = ffi::closest_point_on_polygon(self.pointer, polygon.raw, position)
            .map_err(map_polygon_query)?;
        Ok(self.nearest_from_raw(nearest))
    }

    /// Find a polygon corridor and extract its straight path.
    pub fn find_path(
        &self,
        start: Vec3A,
        end: Vec3A,
        half_extents: Vec3A,
        filter: &QueryFilter,
    ) -> Result<Path, Error> {
        validate_vector(start)?;
        validate_vector(end)?;
        validate_extents(half_extents)?;

        let start = self
            .nearest_point_validated(start, half_extents, filter)?
            .ok_or(Error::StartPolygonNotFound)?;
        let end = self
            .nearest_point_validated(end, half_extents, filter)?
            .ok_or(Error::EndPolygonNotFound)?;
        let mut scratch = self.scratch.borrow_mut();
        let QueryScratch {
            corridor, straight, ..
        } = &mut *scratch;
        let (corridor_count, details) = ffi::find_path(
            self.pointer,
            start.polygon.raw,
            end.polygon.raw,
            start.position,
            end.position,
            filter,
            corridor,
        )
        .map_err(map_query)?;
        reject_path_details(details)?;
        let corridor = &corridor[..corridor_count];
        let Some(&last_polygon) = corridor.last() else {
            return Err(Error::NativeContract);
        };

        let partial = details.partial_result();
        let straight_end = if partial {
            ffi::closest_point_on_polygon(self.pointer, last_polygon, end.position)
                .map_err(map_polygon_query)?
                .position
        } else {
            end.position
        };
        let points = self.straight_path(start.position, straight_end, corridor, straight)?;
        let corridor = corridor
            .iter()
            .copied()
            .map(|raw| self.polygon_ref(raw))
            .collect();
        Ok(Path {
            corridor,
            points,
            partial,
        })
    }

    /// Cast a short horizontal ray over traversable polygons.
    ///
    /// Detour ignores the target's vertical component while traversing the
    /// polygon surface, so this is intended for local visibility checks.
    pub fn raycast(
        &self,
        start: Vec3A,
        end: Vec3A,
        half_extents: Vec3A,
        filter: &QueryFilter,
    ) -> Result<RaycastHit, Error> {
        validate_vector(start)?;
        validate_vector(end)?;
        validate_extents(half_extents)?;
        let start = self
            .nearest_point_validated(start, half_extents, filter)?
            .ok_or(Error::StartPolygonNotFound)?;
        let mut scratch = self.scratch.borrow_mut();
        let (visited_count, details, result) = ffi::raycast(
            self.pointer,
            start.polygon.raw,
            start.position,
            end,
            filter,
            &mut scratch.visited,
        )
        .map_err(map_query)?;
        if details.out_of_nodes() {
            return Err(Error::QueryOutOfNodes);
        }
        if details.buffer_too_small() {
            return Err(Error::RaycastCapacityExceeded);
        }
        if result.fraction.is_nan()
            || result.fraction < 0.0
            || !result.path_cost.is_finite()
            || !result.normal.is_finite()
        {
            return Err(Error::NativeContract);
        }

        let visited = &scratch.visited[..visited_count];
        let hit = result.fraction <= 1.0;
        let fraction = if hit { result.fraction } else { 1.0 };
        let edge_index = if hit {
            u32::try_from(result.edge_index).ok()
        } else {
            None
        };
        Ok(RaycastHit {
            hit,
            fraction,
            position: start.position.lerp(end, fraction),
            normal: if hit { result.normal } else { Vec3A::ZERO },
            edge_index,
            path_cost: result.path_cost,
            visited: visited
                .iter()
                .copied()
                .map(|raw| self.polygon_ref(raw))
                .collect(),
        })
    }

    fn nearest_point_validated(
        &self,
        center: Vec3A,
        half_extents: Vec3A,
        filter: &QueryFilter,
    ) -> Result<Option<NearestPoint>, Error> {
        ffi::find_nearest_point(self.pointer, center, half_extents, filter)
            .map(|nearest| nearest.map(|nearest| self.nearest_from_raw(nearest)))
            .map_err(map_query)
    }

    fn nearest_from_raw(&self, nearest: ffi::Nearest) -> NearestPoint {
        NearestPoint {
            polygon: self.polygon_ref(nearest.polygon),
            position: nearest.position,
            is_over_polygon: nearest.is_over_polygon,
        }
    }

    fn straight_path(
        &self,
        start: Vec3A,
        end: Vec3A,
        corridor: &[u32],
        scratch: &mut StraightPathScratch,
    ) -> Result<Vec<PathPoint>, Error> {
        let (count, details) = ffi::find_straight_path(
            self.pointer,
            start,
            end,
            corridor,
            ffi::StraightPathBuffers {
                points: &mut scratch.positions,
                flags: &mut scratch.flags,
                polygons: &mut scratch.polygons,
            },
        )
        .map_err(map_query)?;
        if details.buffer_too_small() {
            return Err(Error::StraightPathCapacityExceeded);
        }
        if details.out_of_nodes() {
            return Err(Error::QueryOutOfNodes);
        }

        let mut points = Vec::with_capacity(count);
        for index in 0..count {
            let polygon =
                (scratch.polygons[index] != 0).then(|| self.polygon_ref(scratch.polygons[index]));
            points.push(PathPoint {
                position: ffi::safe_vec(scratch.positions[index]),
                polygon,
                kind: path_point_kind(scratch.flags[index]),
            });
        }
        Ok(points)
    }

    fn polygon_ref(&self, raw: u32) -> PolygonRef {
        PolygonRef {
            raw,
            navmesh: self.navmesh.key,
        }
    }

    fn validate_polygon(&self, polygon: PolygonRef) -> Result<(), Error> {
        if polygon.navmesh == self.navmesh.key {
            Ok(())
        } else {
            Err(Error::WrongNavMesh)
        }
    }
}

impl Drop for Query<'_> {
    fn drop(&mut self) {
        ffi::destroy_query(self.pointer);
    }
}

fn validate_vector(value: Vec3A) -> Result<(), Error> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidVector)
    }
}

fn validate_extents(value: Vec3A) -> Result<(), Error> {
    if value.is_finite() && value.x > 0.0 && value.y > 0.0 && value.z > 0.0 {
        Ok(())
    } else {
        Err(Error::InvalidQueryExtents)
    }
}

fn result_capacity(value: NonZeroU32) -> Result<usize, Error> {
    if i32::try_from(value.get()).is_err() {
        return Err(Error::ResultCapacityTooLarge(value.get()));
    }
    usize::try_from(value.get()).map_err(|_error| Error::ResultCapacityTooLarge(value.get()))
}

const fn map_query_initialization(status: Status) -> Error {
    match status {
        Status::OutOfMemory => Error::AllocationFailed,
        Status::InvalidArgument => Error::QueryInitialization,
        Status::InitializationFailed | Status::QueryFailed => Error::QueryInitialization,
        Status::InvalidNavMeshData | Status::TileAlreadyOccupied | Status::ContractViolation => {
            Error::NativeContract
        }
    }
}

const fn map_query(status: Status) -> Error {
    match status {
        Status::OutOfMemory => Error::AllocationFailed,
        Status::QueryFailed => Error::QueryFailed,
        Status::InvalidArgument
        | Status::InvalidNavMeshData
        | Status::InitializationFailed
        | Status::TileAlreadyOccupied
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn map_polygon_query(status: Status) -> Error {
    match status {
        Status::InvalidArgument => Error::InvalidPolygon,
        Status::OutOfMemory => Error::AllocationFailed,
        Status::QueryFailed => Error::QueryFailed,
        Status::InvalidNavMeshData
        | Status::InitializationFailed
        | Status::TileAlreadyOccupied
        | Status::ContractViolation => Error::NativeContract,
    }
}

const fn reject_path_details(details: Details) -> Result<(), Error> {
    if details.out_of_nodes() {
        Err(Error::QueryOutOfNodes)
    } else if details.buffer_too_small() {
        Err(Error::PathCapacityExceeded)
    } else {
        Ok(())
    }
}

fn path_point_kind(flags: u8) -> PathPointKind {
    let flags = u32::from(flags);
    if flags & ffi::raw::BF_NAVIGATION_STRAIGHT_PATH_START != 0 {
        PathPointKind::Start
    } else if flags & ffi::raw::BF_NAVIGATION_STRAIGHT_PATH_END != 0 {
        PathPointKind::End
    } else if flags & ffi::raw::BF_NAVIGATION_STRAIGHT_PATH_OFF_MESH_CONNECTION != 0 {
        PathPointKind::OffMeshConnection
    } else {
        PathPointKind::Corner
    }
}

const fn nonzero(value: u32) -> NonZeroU32 {
    match NonZeroU32::new(value) {
        Some(value) => value,
        None => unreachable!(),
    }
}
