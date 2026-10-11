//! Pure layout contracts. Terminal events and persistence belong to adapters.
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PanelId(String);
impl PanelId {
    pub fn new(value: &str) -> Result<Self, LayoutError> {
        if value.trim().is_empty() {
            return Err(LayoutError::InvalidPanelId);
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: u16,
    pub col: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub row: u16,
    pub col: u16,
    pub height: u16,
    pub width: u16,
}
impl Rect {
    pub fn new(row: u16, col: u16, height: u16, width: u16) -> Result<Self, LayoutError> {
        if height == 0
            || width == 0
            || u32::from(row) + u32::from(height) > 65536
            || u32::from(col) + u32::from(width) > 65536
        {
            return Err(LayoutError::InvalidGeometry);
        }
        Ok(Self {
            row,
            col,
            height,
            width,
        })
    }
    pub fn contains(self, cell: Cell) -> bool {
        u32::from(cell.row) >= u32::from(self.row)
            && u32::from(cell.row) < u32::from(self.row) + u32::from(self.height)
            && u32::from(cell.col) >= u32::from(self.col)
            && u32::from(cell.col) < u32::from(self.col) + u32::from(self.width)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelRect {
    pub id: PanelId,
    pub rect: Rect,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidPanelId,
    InvalidGeometry,
    EmptyLayout,
    EmptyColumn,
    DuplicatePanel,
    TooManyColumns,
    UnknownPanel,
    InvalidTarget,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropTarget {
    Swap(PanelId),
    Move { column: usize, index: usize },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    columns: Vec<Vec<PanelId>>,
    max_columns: usize,
}
impl Layout {
    pub fn new(columns: Vec<Vec<PanelId>>, max_columns: usize) -> Result<Self, LayoutError> {
        if columns.is_empty() {
            return Err(LayoutError::EmptyLayout);
        }
        if columns.len() > max_columns {
            return Err(LayoutError::TooManyColumns);
        }
        let mut ids = BTreeSet::new();
        for column in &columns {
            if column.is_empty() {
                return Err(LayoutError::EmptyColumn);
            }
            for id in column {
                if !ids.insert(id) {
                    return Err(LayoutError::DuplicatePanel);
                }
            }
        }
        Ok(Self {
            columns,
            max_columns,
        })
    }
    pub fn columns(&self) -> &[Vec<PanelId>] {
        &self.columns
    }
    fn location(&self, id: &PanelId) -> Option<(usize, usize)> {
        self.columns
            .iter()
            .enumerate()
            .find_map(|(c, items)| items.iter().position(|p| p == id).map(|i| (c, i)))
    }
    /// Moves use a destination index after source removal, while the column
    /// number refers to the original layout. Empty columns collapse afterward.
    /// This operation returns a new value: persistence is an application concern.
    pub fn apply_drop(&self, dragged: &PanelId, target: &DropTarget) -> Result<Self, LayoutError> {
        let (from, position) = self.location(dragged).ok_or(LayoutError::UnknownPanel)?;
        let mut columns = self.columns.clone();
        match target {
            DropTarget::Swap(other) => {
                let (to, index) = self.location(other).ok_or(LayoutError::UnknownPanel)?;
                columns[from][position] = other.clone();
                columns[to][index] = dragged.clone();
            }
            DropTarget::Move { column, index } => {
                if *column > columns.len() {
                    return Err(LayoutError::InvalidTarget);
                }
                columns[from].remove(position);
                // Index refers to the destination after removal, before empty columns collapse.
                if *column == columns.len() {
                    columns.push(Vec::new());
                }
                if *index > columns[*column].len() {
                    return Err(LayoutError::InvalidTarget);
                }
                columns[*column].insert(*index, dragged.clone());
                columns.retain(|items| !items.is_empty());
            }
        }
        Self::new(columns, self.max_columns)
    }
    pub fn drop_target(
        &self,
        rects: &[PanelRect],
        cell: Cell,
        dragged: &PanelId,
    ) -> Option<DropTarget> {
        self.location(dragged)?;
        if !rects.iter().any(|p| &p.id == dragged) {
            return None;
        }
        let hovered = rects.iter().find(|p| p.rect.contains(cell))?;
        let right = rects
            .iter()
            .map(|p| u32::from(p.rect.col) + u32::from(p.rect.width))
            .max()?;
        let band = (hovered.rect.height / 4).max(1);
        let target = if u32::from(cell.col) + 1 == right && self.columns.len() < self.max_columns {
            DropTarget::Move {
                column: self.columns.len(),
                index: 0,
            }
        } else if cell.row - hovered.rect.row < band
            || u32::from(cell.row)
                >= u32::from(hovered.rect.row) + u32::from(hovered.rect.height) - u32::from(band)
        {
            if &hovered.id == dragged {
                return None;
            }
            let (column, _) = self.location(&hovered.id)?;
            let index = self.columns[column]
                .iter()
                .filter(|p| *p != dragged)
                .position(|p| p == &hovered.id)?;
            DropTarget::Move {
                column,
                index: index + usize::from(cell.row - hovered.rect.row >= band),
            }
        } else {
            if &hovered.id == dragged {
                return None;
            }
            DropTarget::Swap(hovered.id.clone())
        };
        let changed = self.apply_drop(dragged, &target).ok()?;
        if changed == *self {
            None
        } else {
            Some(target)
        }
    }
}
pub fn panel_at(rects: &[PanelRect], cell: Cell) -> Option<&PanelId> {
    rects.iter().find(|p| p.rect.contains(cell)).map(|p| &p.id)
}
pub fn title_at(rects: &[PanelRect], cell: Cell) -> Option<&PanelId> {
    rects
        .iter()
        .find(|p| p.rect.row == cell.row && p.rect.contains(cell))
        .map(|p| &p.id)
}

pub fn neighbor<'a>(
    rects: &'a [PanelRect],
    id: &PanelId,
    direction: Direction,
) -> Option<&'a PanelId> {
    let origin = rects.iter().find(|p| &p.id == id)?.rect;
    rects
        .iter()
        .filter(|p| &p.id != id)
        .filter_map(|p| {
            let a = origin;
            let b = p.rect;
            let (gap, overlap, center) = match direction {
                Direction::Up | Direction::Down => (
                    if direction == Direction::Up {
                        i32::from(a.row) - i32::from(b.row) - i32::from(b.height)
                    } else {
                        i32::from(b.row) - i32::from(a.row) - i32::from(a.height)
                    },
                    (i32::from(a.col) + i32::from(a.width))
                        .min(i32::from(b.col) + i32::from(b.width))
                        - i32::from(a.col).max(i32::from(b.col)),
                    (2 * i32::from(a.col) + i32::from(a.width)
                        - 2 * i32::from(b.col)
                        - i32::from(b.width))
                    .abs(),
                ),
                Direction::Left | Direction::Right => (
                    if direction == Direction::Left {
                        i32::from(a.col) - i32::from(b.col) - i32::from(b.width)
                    } else {
                        i32::from(b.col) - i32::from(a.col) - i32::from(a.width)
                    },
                    (i32::from(a.row) + i32::from(a.height))
                        .min(i32::from(b.row) + i32::from(b.height))
                        - i32::from(a.row).max(i32::from(b.row)),
                    (2 * i32::from(a.row) + i32::from(a.height)
                        - 2 * i32::from(b.row)
                        - i32::from(b.height))
                    .abs(),
                ),
            };
            if gap < 0 {
                None
            } else {
                Some(((overlap <= 0, gap, -overlap, center, &p.id), &p.id))
            }
        })
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|p| p.1)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragPreview {
    pub source: PanelId,
    pub cursor: Cell,
    pub target: Option<DropTarget>,
}
#[derive(Clone, Debug)]
pub struct DragSession {
    source: PanelId,
}
impl DragSession {
    pub fn start(layout: &Layout, rects: &[PanelRect], cell: Cell) -> Option<Self> {
        let source = title_at(rects, cell)?.clone();
        layout.location(&source)?;
        Some(Self { source })
    }
    pub fn update(&self, layout: &Layout, rects: &[PanelRect], cell: Cell) -> DragPreview {
        DragPreview {
            source: self.source.clone(),
            cursor: cell,
            target: layout.drop_target(rects, cell, &self.source),
        }
    }
    pub fn finish(
        self,
        layout: &Layout,
        rects: &[PanelRect],
        cell: Cell,
    ) -> Result<Option<Layout>, LayoutError> {
        layout
            .drop_target(rects, cell, &self.source)
            .map(|target| layout.apply_drop(&self.source, &target))
            .transpose()
    }
    pub fn cancel(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(s: &str) -> PanelId {
        PanelId::new(s).unwrap()
    }
    fn fixture() -> (Layout, Vec<PanelRect>) {
        (
            Layout::new(vec![vec![id("a")], vec![id("b"), id("c")]], 3).unwrap(),
            vec![
                PanelRect {
                    id: id("a"),
                    rect: Rect::new(0, 0, 12, 10).unwrap(),
                },
                PanelRect {
                    id: id("b"),
                    rect: Rect::new(0, 10, 6, 10).unwrap(),
                },
                PanelRect {
                    id: id("c"),
                    rect: Rect::new(6, 10, 6, 10).unwrap(),
                },
            ],
        )
    }
    #[test]
    fn validates_identity_and_structure() {
        assert_eq!(PanelId::new(" "), Err(LayoutError::InvalidPanelId));
        assert_eq!(
            Layout::new(vec![vec![id("a"), id("a")]], 2),
            Err(LayoutError::DuplicatePanel)
        );
        assert_eq!(Rect::new(65535, 0, 2, 1), Err(LayoutError::InvalidGeometry));
    }
    #[test]
    fn swap_and_move_keep_each_panel_once() {
        let (layout, _) = fixture();
        assert_eq!(
            layout
                .apply_drop(&id("a"), &DropTarget::Swap(id("b")))
                .unwrap()
                .columns()[0],
            vec![id("b")]
        );
        let moved = layout
            .apply_drop(
                &id("a"),
                &DropTarget::Move {
                    column: 1,
                    index: 1,
                },
            )
            .unwrap();
        assert_eq!(moved.columns(), &[vec![id("b"), id("a"), id("c")]]);
        assert_eq!(
            layout.apply_drop(
                &id("a"),
                &DropTarget::Move {
                    column: 4,
                    index: 0
                }
            ),
            Err(LayoutError::InvalidTarget)
        );
    }
    #[test]
    fn preview_does_not_commit_and_release_uses_current_cell() {
        let (layout, rects) = fixture();
        let before = layout.clone();
        let drag = DragSession::start(&layout, &rects, Cell { row: 0, col: 2 }).unwrap();
        assert_eq!(
            drag.update(&layout, &rects, Cell { row: 3, col: 12 })
                .target,
            Some(DropTarget::Swap(id("b")))
        );
        assert_eq!(layout, before);
        assert!(drag
            .finish(&layout, &rects, Cell { row: 30, col: 30 })
            .unwrap()
            .is_none());
        assert!(DragSession::start(&layout, &rects, Cell { row: 3, col: 2 }).is_none());
    }
    #[test]
    fn drop_bands_and_right_edge_have_explicit_targets() {
        let (layout, rects) = fixture();
        assert_eq!(
            layout.drop_target(&rects, Cell { row: 0, col: 12 }, &id("a")),
            Some(DropTarget::Move {
                column: 1,
                index: 0
            })
        );
        assert_eq!(
            layout.drop_target(&rects, Cell { row: 5, col: 12 }, &id("a")),
            Some(DropTarget::Move {
                column: 1,
                index: 1
            })
        );
        assert_eq!(
            layout.drop_target(&rects, Cell { row: 3, col: 19 }, &id("a")),
            Some(DropTarget::Move {
                column: 2,
                index: 0
            })
        );
        assert!(layout
            .drop_target(&rects, Cell { row: 3, col: 2 }, &id("a"))
            .is_none());
    }
    #[test]
    fn neighbor_prefers_aligned_then_nearby_and_is_deterministic() {
        let (_, rects) = fixture();
        assert_eq!(neighbor(&rects, &id("a"), Direction::Right), Some(&id("b")));
        assert_eq!(neighbor(&rects, &id("b"), Direction::Down), Some(&id("c")));
        assert!(neighbor(&rects, &id("a"), Direction::Left).is_none());
    }
    #[test]
    fn reorder_within_column_uses_post_removal_index() {
        let layout = Layout::new(vec![vec![id("a"), id("b"), id("c")]], 2).unwrap();
        let moved = layout
            .apply_drop(
                &id("a"),
                &DropTarget::Move {
                    column: 0,
                    index: 2,
                },
            )
            .unwrap();
        assert_eq!(moved.columns(), &[vec![id("b"), id("c"), id("a")]]);
        assert_eq!(
            layout.apply_drop(
                &id("a"),
                &DropTarget::Move {
                    column: 0,
                    index: 3
                }
            ),
            Err(LayoutError::InvalidTarget)
        );
    }
    #[test]
    fn reject_extra_column_without_losing_source() {
        let layout = Layout::new(vec![vec![id("a"), id("b")]], 1).unwrap();
        assert_eq!(
            layout.apply_drop(
                &id("a"),
                &DropTarget::Move {
                    column: 1,
                    index: 0
                }
            ),
            Err(LayoutError::TooManyColumns)
        );
        assert_eq!(layout.columns(), &[vec![id("a"), id("b")]]);
        assert_eq!(
            layout.apply_drop(&id("missing"), &DropTarget::Swap(id("a"))),
            Err(LayoutError::UnknownPanel)
        );
    }
}
