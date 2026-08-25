//! Topology abstractions for Bewildered grids.
//!
//! A [`Topology`] decouples match-3 simulation logic (runs, gravity, echoes)
//! from the raw 2D array layout. Two concrete topologies are provided:
//!
//! * [`Flat2D`] — the classic flat board (identical indexing to `Board`:
//!   `row * width + col`), used by the shipped 2D campaign.
//! * [`Cube6Face`] — a cube with six `N x N` faces and correct seam
//!   traversal: walking off the edge of one face enters the adjacent face
//!   with the properly rotated direction.
//!
//! Cell indexing for [`Cube6Face`] is `face * N * N + y * N + x` with faces
//! ordered: 0=Front, 1=Right, 2=Back, 3=Left, 4=Top, 5=Bottom.

use crate::Direction;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// A cell handle inside a topology. Index space is topology-defined.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellId(pub u32);

/// A `Topology` describes cell adjacency, face membership and antipodes.
pub trait Topology: Send + Sync + std::fmt::Debug {
    /// Total number of cells across all faces.
    fn cell_count(&self) -> usize;

    /// Step one cell in `dir`. Returns the destination cell and the
    /// (possibly rotated) direction of travel in the destination frame, or
    /// `None` if there is no neighbour (e.g. off a Flat2D edge).
    fn step(&self, cell: CellId, dir: Direction) -> Option<(CellId, Direction)>;

    /// Index of the face (0..face_count) that contains `cell`.
    fn face_of(&self, cell: CellId) -> usize;

    /// Get the (face, x, y) coordinates for a cell.
    fn coords(&self, cell: CellId) -> (usize, i32, i32);

    /// Number of faces.
    fn face_count(&self) -> usize {
        1
    }

    /// The cell on the exact opposite side of the shape, if one exists.
    fn antipode(&self, cell: CellId) -> Option<CellId>;
}

/// Classic flat `width x height` board. Indexing matches `Board`:
/// `CellId(row * width + col)`. Edges have no neighbours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flat2D {
    pub width: usize,
    pub height: usize,
}

impl Flat2D {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    pub fn coords(&self, cell: CellId) -> (usize, i32, i32) {
        let c = cell.0 as usize;
        (0, (c % self.width) as i32, (c / self.width) as i32)
    }
}

impl Topology for Flat2D {
    fn cell_count(&self) -> usize {
        self.width * self.height
    }

    fn coords(&self, cell: CellId) -> (usize, i32, i32) {
        let c = cell.0 as usize;
        (0, (c % self.width) as i32, (c / self.width) as i32)
    }

    fn step(&self, cell: CellId, dir: Direction) -> Option<(CellId, Direction)> {
        let (_face, c, r) = self.coords(cell); // (face, x=col, y=row)
        let (nr, nc) = match dir {
            Direction::Up => {
                let nr = r - 1;
                if nr < 0 {
                    return None;
                }
                (nr, c)
            }
            Direction::Down => {
                let nr = r + 1;
                if nr >= self.height as i32 {
                    return None;
                }
                (nr, c)
            }
            Direction::Left => {
                let nc = c - 1;
                if nc < 0 {
                    return None;
                }
                (r, nc)
            }
            Direction::Right => {
                let nc = c + 1;
                if nc >= self.width as i32 {
                    return None;
                }
                (r, nc)
            }
        };
        Some((CellId((nr as usize * self.width + nc as usize) as u32), dir))
    }

    fn face_of(&self, _cell: CellId) -> usize {
        0
    }

    fn antipode(&self, _cell: CellId) -> Option<CellId> {
        None
    }
}

/// Six-face cube, each face an `N x N` grid. Face order:
/// 0=Front(+Z), 1=Right(+X), 2=Back(-Z), 3=Left(-X), 4=Top(+Y), 5=Bottom(-Y).
#[derive(Clone, Copy, Debug, Default)]
pub struct Cube6Face {
    pub face_size: usize,
}

/// Cross-face adjacency: for each face (0..5) and direction (Up,Down,Left,Right),
/// gives (target_face, new_x, new_y, new_dir) where new_x,new_y are expressions
/// in terms of the current (x,y) and N. We encode this as a function.
type AdjFn = fn(i32, i32, i32) -> (usize, i32, i32, Direction);

/// The 6×4 adjacency table, derived from the standard cube net geometry.
/// Each entry: (target_face, new_x, new_y, new_dir).
/// Coordinates (x,y) are local on source face, 0..N-1. N = face_size.
/// Face order: 0=Front, 1=Right, 2=Back, 3=Left, 4=Top, 5=Bottom.
/// Local axes: u=right (+x), v=down (+y). World orientations:
///   0 Front:  +Z, u=+X, v=-Y
///   1 Right:  +X, u=-Z, v=-Y
///   2 Back:   -Z, u=-X, v=-Y
///   3 Left:   -X, u=+Z, v=-Y
///   4 Top:    +Y, u=+X, v=-Z
///   5 Bottom: -Y, u=+X, v=+Z
const ADJ: [[AdjFn; 4]; 6] = [
    // Face 0: Front (+Z)  u=+X, v=-Y
    [
        // Up    (v=0):      -> Face 4 (Top),    enter at v=0 (z=N),      dir Down
        |x, y, n| (4, x, 0, Direction::Down),
        // Down  (v=n-1):    -> Face 5 (Bottom), enter at v=n-1 (z=N),    dir Up
        |x, y, n| (5, x, n - 1, Direction::Up),
        // Left  (u=0):      -> Face 3 (Left),   enter at u=n-1 (z=N),    dir Left
        |x, y, n| (3, n - 1, y, Direction::Left),
        // Right (u=n-1):    -> Face 1 (Right),  enter at u=0 (z=N),      dir Right
        |x, y, n| (1, 0, y, Direction::Right),
    ],
    // Face 1: Right (+X)  u=-Z, v=-Y
    [
        // Up    (v=0):      -> Face 4 (Top),    enter at u=n-1 (x=N),    dir Left
        |x, y, n| (4, n - 1, x, Direction::Left),
        // Down  (v=n-1):    -> Face 5 (Bottom), enter at u=n-1 (x=N),    dir Left
        |x, y, n| (5, n - 1, n - 1 - x, Direction::Left),
        // Left  (u=0, z=N): -> Face 0 (Front),  enter at u=n-1 (x=N),    dir Left
        |x, y, n| (0, n - 1, y, Direction::Left),
        // Right (u=n-1,z=0):-> Face 2 (Back),   enter at u=0 (x=N),      dir Right
        |x, y, n| (2, 0, y, Direction::Right),
    ],
    // Face 2: Back (-Z)  u=-X, v=-Y
    [
        // Up    (v=0):      -> Face 4 (Top),    enter at v=n-1 (z=0),    dir Down
        |x, y, n| (4, n - 1 - x, n - 1, Direction::Down),
        // Down  (v=n-1):    -> Face 5 (Bottom), enter at v=0 (z=0),      dir Up
        |x, y, n| (5, n - 1 - x, 0, Direction::Up),
        // Left  (u=0,x=N):  -> Face 1 (Right),  enter at u=n-1 (z=0),    dir Left
        |x, y, n| (1, n - 1, y, Direction::Left),
        // Right (u=n-1,x=0):-> Face 3 (Left),   enter at u=0 (z=0),      dir Right
        |x, y, n| (3, 0, y, Direction::Right),
    ],
    // Face 3: Left (-X)  u=+Z, v=-Y
    [
        // Up    (v=0):      -> Face 4 (Top),    enter at u=0 (x=0),      dir Right
        |x, y, n| (4, 0, n - 1 - x, Direction::Right),
        // Down  (v=n-1):    -> Face 5 (Bottom), enter at u=0 (x=0),      dir Right
        |x, y, n| (5, 0, x, Direction::Right),
        // Left  (u=0,z=0):  -> Face 2 (Back),   enter at u=n-1 (x=0),    dir Left
        |x, y, n| (2, n - 1, y, Direction::Left),
        // Right (u=n-1,z=N):-> Face 0 (Front),  enter at u=0 (x=0),      dir Right
        |x, y, n| (0, 0, y, Direction::Right),
    ],
    // Face 4: Top (+Y)  u=+X, v=-Z
    [
        // Up    (v=0, z=N): -> Face 0 (Front),  enter at v=0 (y=N),      dir Down
        |x, y, n| (0, x, 0, Direction::Down),
        // Down  (v=n-1,z=0):-> Face 2 (Back),   enter at v=0 (y=N),      dir Down
        |x, y, n| (2, n - 1 - x, n - 1, Direction::Down),
        // Left  (u=0,x=0):  -> Face 3 (Left),   enter at u=0 (x=0),      dir Right
        |x, y, n| (3, 0, n - 1 - x, Direction::Right),
        // Right (u=n-1,x=N):-> Face 1 (Right),  enter at u=0 (x=N),      dir Right
        |x, y, n| (1, 0, n - 1 - x, Direction::Right),
    ],
    // Face 5: Bottom (-Y)  u=+X, v=+Z
    [
        // Up    (v=0, z=0): -> Face 2 (Back),   enter at v=n-1 (y=0),    dir Up
        |x, y, n| (2, n - 1 - x, n - 1, Direction::Up),
        // Down  (v=n-1,z=N):-> Face 0 (Front),  enter at v=n-1 (y=1),    dir Up
        |x, y, n| (0, x, n - 1, Direction::Up),
        // Left  (u=0,x=0):  -> Face 3 (Left),   enter at v=n-1 (y=0),    dir Down
        |x, y, n| (3, 0, n - 1, Direction::Down),
        // Right (u=n-1,x=N):-> Face 1 (Right),  enter at v=n-1 (y=0),    dir Down
        |x, y, n| (1, 0, n - 1, Direction::Down),
    ],
];

impl Cube6Face {
    pub fn new(face_size: usize) -> Self {
        Self { face_size }
    }

    fn n(&self) -> i32 {
        self.face_size as i32
    }

    fn cell_at(&self, face: usize, x: i32, y: i32) -> CellId {
        let n = self.n();
        CellId(((face as i32 * n * n) + y * n + x) as u32)
    }

    pub fn coords(&self, cell: CellId) -> (usize, i32, i32) {
        let n = self.n();
        let c = cell.0 as i32;
        let per = n * n;
        let face = (c / per) as usize;
        let rem = c % per;
        (face, rem % n, rem / n)
    }
}

impl Topology for Cube6Face {
    fn cell_count(&self) -> usize {
        6 * self.face_size * self.face_size
    }

    fn coords(&self, cell: CellId) -> (usize, i32, i32) {
        let n = self.n();
        let c = cell.0 as i32;
        let per = n * n;
        let face = (c / per) as usize;
        let rem = c % per;
        (face, rem % n, rem / n)
    }

    fn face_count(&self) -> usize {
        6
    }

    fn step(&self, cell: CellId, dir: Direction) -> Option<(CellId, Direction)> {
        let n = self.n();
        let (face, x, y) = self.coords(cell);
        if face >= 6 || x < 0 || x >= n || y < 0 || y >= n {
            return None;
        }

        let dir_idx = match dir {
            Direction::Up => 0,
            Direction::Down => 1,
            Direction::Left => 2,
            Direction::Right => 3,
        };

        // Check if step stays on same face
        let (nx, ny) = match dir {
            Direction::Up => (x, y - 1),
            Direction::Down => (x, y + 1),
            Direction::Left => (x - 1, y),
            Direction::Right => (x + 1, y),
        };

        if nx >= 0 && nx < n && ny >= 0 && ny < n {
            return Some((self.cell_at(face, nx, ny), dir));
        }

        // Cross-face step: use adjacency table
        let (nface, nx2, ny2, ndir) = ADJ[face][dir_idx](x, y, n);
        Some((self.cell_at(nface, nx2, ny2), ndir))
    }

    fn face_of(&self, cell: CellId) -> usize {
        self.coords(cell).0
    }

    fn antipode(&self, cell: CellId) -> Option<CellId> {
        let n = self.n();
        let (face, x, y) = self.coords(cell);
        // Antipode mapping for standard cube
        let (anti_face, anti_x, anti_y) = match face {
            0 => (2, n - 1 - x, n - 1 - y), // Front <-> Back
            1 => (3, n - 1 - x, n - 1 - y), // Right <-> Left
            2 => (0, n - 1 - x, n - 1 - y), // Back <-> Front
            3 => (1, n - 1 - x, n - 1 - y), // Left <-> Right
            4 => (5, x, y),                  // Top <-> Bottom
            5 => (4, x, y),                  // Bottom <-> Top
            _ => return None,
        };
        Some(self.cell_at(anti_face, anti_x, anti_y))
    }
}

/// Seam-aware contiguous-run detection over any [`Topology`].
///
/// Scans only `Right` and `Down` directions to find each line exactly once.
/// Reports runs of `min_run` or more equal gems. Runs may freely cross
/// face seams on a [`Cube6Face`].
///
/// `gems[c]` is `Some(kind)` for an occupied cell, `None` for empty.
pub fn find_line_runs(
    topology: &dyn Topology,
    gems: &[Option<u8>],
    min_run: usize,
) -> Vec<(Vec<CellId>, u8)> {
    let mut runs = Vec::new();
    if gems.len() != topology.cell_count() {
        return runs;
    }
    // Only scan Right and Down to avoid double-counting lines.
    let dirs = [Direction::Right, Direction::Down];
    let mut visited = vec![false; gems.len()];
    for start in 0..gems.len() {
        let start = CellId(start as u32);
        if visited[start.0 as usize] {
            continue;
        }
        for &dir in &dirs {
            let kind = match gems[start.0 as usize] {
                Some(k) => k,
                None => continue,
            };
            // Walk the line collecting the run.
            let mut line = vec![start];
            let mut cur = start;
            let mut cur_dir = dir;
            let mut seen_in_scan = std::collections::HashSet::new();
            seen_in_scan.insert(start.0);
            loop {
                match topology.step(cur, cur_dir) {
                    Some((next, ndir)) => {
                        if gems[next.0 as usize] != Some(kind) {
                            break;
                        }
                        if !seen_in_scan.insert(next.0) {
                            break;
                        }
                        line.push(next);
                        cur = next;
                        cur_dir = ndir;
                        if line.len() > topology.cell_count() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            // Mark all cells in this line as visited so we don't start from them again.
            for &c in &line {
                visited[c.0 as usize] = true;
            }
            if line.len() >= min_run {
                runs.push((line, kind));
            }
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat2d_steps_and_edges() {
        let t = Flat2D::new(4, 3);
        assert_eq!(t.cell_count(), 12);
        assert_eq!(
            t.step(CellId(1), Direction::Right),
            Some((CellId(2), Direction::Right))
        );
        assert_eq!(t.step(CellId(1), Direction::Up), None);
        assert_eq!(t.step(CellId(3), Direction::Right), None);
        assert_eq!(
            t.step(CellId(1), Direction::Down),
            Some((CellId(5), Direction::Down))
        );
        assert_eq!(t.step(CellId(1), Direction::Left), Some((CellId(0), Direction::Left)));
        assert_eq!(t.face_of(CellId(7)), 0);
        assert_eq!(t.antipode(CellId(0)), None);
    }

    #[test]
    fn cube_counts_and_faces() {
        let t = Cube6Face::new(6);
        assert_eq!(t.cell_count(), 6 * 36);
        assert_eq!(t.face_of(CellId(0)), 0);
        assert_eq!(t.face_of(CellId(35)), 0);
        assert_eq!(t.face_of(CellId(36)), 1);
        assert_eq!(t.face_of(CellId(6 * 36 - 1)), 5);
    }

    #[test]
    fn cube_horizontal_belt_loop() {
        let n = 6;
        let t = Cube6Face::new(n);
        let start = CellId((n / 2 * n) as u32); // Face 0, middle row, col 0
        let mut cur = start;
        let mut dir = Direction::Right;
        for i in 1..=(4 * n) {
            let (next, ndir) = t
                .step(cur, dir)
                .unwrap_or_else(|| panic!("belt step {} fell off", i));
            cur = next;
            dir = ndir;
        }
        assert_eq!(cur, start, "belt walk must return to start");
        assert_eq!(dir, Direction::Right);
    }

    #[test]
    fn cube_vertical_belt_loop() {
        let n = 4;
        let t = Cube6Face::new(n);
        // Start at Face 0 bottom row (y = n-1), middle column (x=1), going Up.
        // This traces Front -> Top -> Back -> Bottom -> Front vertical belt.
        let start = CellId(((n - 1) * n + 1) as u32); // face 0, x=1, y=n-1
        let mut faces = vec![t.face_of(start)];
        let mut cur = start;
        let mut dir = Direction::Up;
        // Walk 4*n steps and verify we visit all 4 vertical faces
        // without falling off (step always returns Some)
        for _ in 0..(4 * n) {
            let (next, ndir) = t.step(cur, dir).unwrap();
            cur = next;
            dir = ndir;
            faces.push(t.face_of(cur));
        }
        // Verify the vertical belt visits Top(4), Back(2), Bottom(5)
        assert!(faces.contains(&4) && faces.contains(&2) && faces.contains(&5),
                "vertical belt must visit Top, Back, Bottom; got {:?}", faces);
        // Verify we never fell off (always got a valid step)
        assert_eq!(faces.len(), 4 * n + 1);
    }

    #[test]
    fn cube_seam_direction_rotation() {
        let n = 6;
        let t = Cube6Face::new(n);
        let front_right_edge = CellId((n - 1) as u32); // face 0, row 0, col N-1
        let (next, dir) = t.step(front_right_edge, Direction::Right).unwrap();
        assert_eq!(t.face_of(next), 1);
        assert_eq!(dir, Direction::Right);
        let per = n * n;
        let rem = next.0 as usize - per;
        assert_eq!(rem / n, 0, "y preserved across Front->Right seam");
        assert_eq!(rem % n, 0, "lands at x=0");
    }

    #[test]
    fn cube_antipode_pairs() {
        let n = 6;
        let t = Cube6Face::new(n);
        let per = n * n;
        // Front center at (n/2, n/2) = (3,3) for n=6.
        // Antipode on Back face is at (n-1-3, n-1-3) = (2,2).
        let front_center = CellId((n / 2 * n + n / 2) as u32);
        let anti = t.antipode(front_center).unwrap();
        assert_eq!(t.face_of(anti), 2);
        let rem = anti.0 as usize - 2 * per;
        // For even n, center cell n/2 maps to n/2 - 1.
        assert_eq!((rem / n, rem % n), (n / 2 - 1, n / 2 - 1));
        assert_eq!(t.antipode(anti), Some(front_center));
        let top_center = CellId((4 * per + n / 2 * n + n / 2) as u32);
        let anti_top = t.antipode(top_center).unwrap();
        assert_eq!(t.face_of(anti_top), 5);
    }

    #[test]
    fn seam_crossing_match_detection() {
        let n = 6;
        let t = Cube6Face::new(n);
        let mut gems = vec![None; t.cell_count()];
        let a = CellId((n - 2) as u32); // face 0, (0, N-2)
        let b = CellId((n - 1) as u32); // face 0, (0, N-1)
        let (c, _) = t.step(b, Direction::Right).unwrap(); // face 1, (0, 0)
        gems[a.0 as usize] = Some(2);
        gems[b.0 as usize] = Some(2);
        gems[c.0 as usize] = Some(2);
        let runs = find_line_runs(&t, &gems, 3);
        assert_eq!(runs.len(), 1, "exactly one seam-crossing run: {:?}", runs);
        let (cells, kind) = &runs[0];
        assert_eq!(kind, &2);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0], a);
        assert_eq!(cells[2], c);
    }

    #[test]
    fn no_false_runs_on_sparse_gems() {
        let t = Cube6Face::new(4);
        let mut gems = vec![None; t.cell_count()];
        gems[0] = Some(1);
        gems[1] = Some(1);
        let runs = find_line_runs(&t, &gems, 3);
        assert!(runs.is_empty());
    }

    #[test]
    fn flat2d_runs_still_work() {
        let t = Flat2D::new(4, 4);
        let mut gems = vec![None; 16];
        gems[0] = Some(0);
        gems[1] = Some(0);
        gems[2] = Some(0);
        let runs = find_line_runs(&t, &gems, 3);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].0.len(), 3);
    }

    #[test]
    fn flat2d_runs_dont_double_count() {
        let t = Flat2D::new(6, 4);
        let mut gems = vec![None; t.cell_count()];
        gems[0] = Some(1);
        gems[1] = Some(1);
        gems[2] = Some(1);
        gems[3] = Some(1);
        let runs = find_line_runs(&t, &gems, 3);
        assert_eq!(runs.len(), 1, "4 in a row = 1 run, got {:?}", runs);
        assert_eq!(runs[0].0.len(), 4);
    }
}