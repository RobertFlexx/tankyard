//! A complete, deterministic tier layout. Geometry is built only when content changes.
use tankyard_content::Content;
use tankyard_core::V2;

/// Horizontal distance between card centers.
pub const COLUMN: f32 = 124.;
/// Vertical distance between wrapped rows of one tier.
pub const ROW: f32 = 100.;
/// Space between the last row of one tier and the next tier.
const TIER_GAP: f32 = 172.;
const MAX_COLUMNS: usize = 28;

#[derive(Default)]
pub struct Layout {
    pub nodes: Vec<V2>,
    pub edges: Vec<(usize, usize)>,
    /// First and last row y and the level requirement of each tier.
    pub tiers: Vec<(f32, f32, u8)>,
    /// Each node's index into `tiers`.
    pub band: Vec<usize>,
    /// Whether a card of the same tier sits directly above this node.
    pub covered: Vec<bool>,
    /// Whether a card of the same tier sits directly below this node.
    pub blocked: Vec<bool>,
    /// A parent's position within its tier, used to stagger wiring buses.
    pub lane: Vec<usize>,
    pub width: f32,
    pub height: f32,
}
impl Layout {
    pub fn new(content: &Content) -> Self {
        let count = content.classes.len();
        let mut result = Self {
            nodes: vec![V2::ZERO; count],
            band: vec![0; count],
            lane: vec![0; count],
            covered: vec![false; count],
            blocked: vec![false; count],
            ..Self::default()
        };
        let index_of = |id: &str| content.classes.iter().position(|c| c.id == id);
        let parents: Vec<Vec<usize>> = content
            .classes
            .iter()
            .map(|c| c.parents.iter().filter_map(|id| index_of(id)).collect())
            .collect();
        let tiers = content.classes.iter().map(|c| c.tier).max().unwrap_or(0);
        let mut rows_by_tier: Vec<Vec<usize>> = Vec::new();
        let mut top = 0.;
        for tier in 0..=tiers {
            let mut row: Vec<_> = (0..count)
                .filter(|&i| content.classes[i].tier == tier)
                .collect();
            if row.is_empty() {
                continue;
            }
            // Order by the parents' mean position so branches stay together;
            // names break ties deterministically.
            let center = |index: usize| {
                let list = &parents[index];
                list.iter().map(|&p| result.nodes[p].x).sum::<f32>() / list.len().max(1) as f32
            };
            row.sort_by(|&a, &b| {
                center(a)
                    .total_cmp(&center(b))
                    .then_with(|| content.classes[a].name.cmp(&content.classes[b].name))
            });
            let rows = row.len().div_ceil(MAX_COLUMNS).max(1);
            let columns = row.len().div_ceil(rows);
            result.tiers.push((
                top,
                top + (rows - 1) as f32 * ROW,
                content.classes[row[0]].min_level,
            ));
            let band = result.tiers.len() - 1;
            // Fill wrapped tiers column by column: siblings stay beneath
            // their parent instead of being spread across the whole width.
            for (slot, &index) in row.iter().enumerate() {
                let (column, line) = (slot / rows, slot % rows);
                result.nodes[index] = V2::new(
                    (column as f32 - columns.saturating_sub(1) as f32 * 0.5) * COLUMN,
                    top + line as f32 * ROW,
                );
                result.covered[index] = line > 0;
                result.blocked[index] = line + 1 < rows && slot + 1 < row.len();
                result.band[index] = band;
            }
            top += (rows - 1) as f32 * ROW + TIER_GAP;
            rows_by_tier.push(row);
        }
        // The widest tier anchors the drawing. Tiers above it sit over their
        // children and tiers below it under their parents, like a family tree.
        let widest = (0..rows_by_tier.len())
            .max_by_key(|&band| {
                let row = &rows_by_tier[band];
                (
                    row.len().div_ceil(row.len().div_ceil(MAX_COLUMNS).max(1)),
                    band,
                )
            })
            .unwrap_or(0);
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (child, list) in parents.iter().enumerate() {
            for &parent in list {
                children[parent].push(child);
            }
        }
        let mean = |nodes: &[V2], list: &[usize], fallback: f32| {
            if list.is_empty() {
                fallback
            } else {
                list.iter().map(|&i| nodes[i].x).sum::<f32>() / list.len() as f32
            }
        };
        for row in rows_by_tier.iter_mut().skip(widest + 1) {
            if row.len() > MAX_COLUMNS {
                continue;
            }
            let desired: Vec<_> = row
                .iter()
                .map(|&i| mean(&result.nodes, &parents[i], result.nodes[i].x))
                .collect();
            place(&mut result.nodes, row, &desired);
        }
        for band in (0..widest).rev() {
            if rows_by_tier[band].len() > MAX_COLUMNS {
                continue;
            }
            let desired: Vec<_> = rows_by_tier[band]
                .iter()
                .map(|&i| {
                    let below: Vec<_> = children[i]
                        .iter()
                        .copied()
                        .filter(|&c| result.band[c] == band + 1)
                        .collect();
                    mean(&result.nodes, &below, result.nodes[i].x)
                })
                .collect();
            place(&mut result.nodes, &mut rows_by_tier[band], &desired);
        }
        for row in &rows_by_tier {
            let mut ordered = row.clone();
            ordered.sort_by(|&a, &b| result.nodes[a].x.total_cmp(&result.nodes[b].x));
            for (lane, index) in ordered.into_iter().enumerate() {
                result.lane[index] = lane;
            }
        }
        let (left, right) = result
            .nodes
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, r), node| {
                (l.min(node.x), r.max(node.x))
            });
        if left <= right {
            let shift = (left + right) * 0.5;
            for node in &mut result.nodes {
                node.x -= shift;
            }
            result.width = right - left + COLUMN;
        }
        for (child, list) in parents.iter().enumerate() {
            for &parent in list {
                result.edges.push((parent, child));
            }
        }
        result.height = (top - TIER_GAP).max(0.) + ROW;
        result
    }
}
/// Put one row's cards as close to their desired x as spacing allows,
/// keeping their order and the row's overall center.
fn place(nodes: &mut [V2], row: &mut [usize], desired: &[f32]) {
    let mut order: Vec<_> = (0..row.len()).collect();
    order.sort_by(|&a, &b| desired[a].total_cmp(&desired[b]).then(a.cmp(&b)));
    let wanted: Vec<_> = order.iter().map(|&k| desired[k]).collect();
    let sorted: Vec<_> = order.iter().map(|&k| row[k]).collect();
    let mut placed: Vec<f32> = Vec::with_capacity(wanted.len());
    for &x in &wanted {
        let x = placed
            .last()
            .map_or(x, |&previous| x.max(previous + COLUMN));
        placed.push(x);
    }
    let drift =
        wanted.iter().zip(&placed).map(|(w, p)| w - p).sum::<f32>() / placed.len().max(1) as f32;
    for (&index, x) in sorted.iter().zip(placed) {
        nodes[index].x = x + drift;
    }
    row.copy_from_slice(&sorted);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_tank_and_connection_is_present_without_overlapping_cards() {
        let content = Content::builtin();
        let tree = Layout::new(&content);
        assert_eq!(tree.nodes.len(), content.classes.len());
        assert_eq!(
            tree.edges.len(),
            content
                .classes
                .iter()
                .map(|c| c.parents.len())
                .sum::<usize>()
        );
        for (i, a) in tree.nodes.iter().enumerate() {
            for b in tree.nodes.iter().skip(i + 1) {
                assert!((a.x - b.x).abs() >= 120. || (a.y - b.y).abs() >= 100.);
            }
        }
        for &(parent, child) in &tree.edges {
            assert!(tree.nodes[parent].y < tree.nodes[child].y);
        }
        assert_eq!(tree.tiers.len(), 5);
    }

    #[test]
    fn wrapped_tiers_keep_children_near_their_parents() {
        let content = Content::builtin();
        let tree = Layout::new(&content);
        // Mean horizontal wire length stays within a few columns.
        let total: f32 = tree
            .edges
            .iter()
            .filter(|&&(_, child)| content.classes[child].tier <= 3)
            .map(|&(parent, child)| (tree.nodes[parent].x - tree.nodes[child].x).abs())
            .sum();
        let edges = tree
            .edges
            .iter()
            .filter(|&&(_, child)| content.classes[child].tier <= 3)
            .count();
        assert!(
            total / (edges as f32) < COLUMN * 3.,
            "{}",
            total / edges as f32
        );
    }
}
