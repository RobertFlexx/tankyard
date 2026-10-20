//! A complete, deterministic tier layout. Geometry is built only when content changes.
use tankyard_content::Content;
use tankyard_core::V2;

#[derive(Default)]
pub struct Layout {
    pub nodes: Vec<V2>,
    pub edges: Vec<(usize, usize)>,
    pub width: f32,
    pub height: f32,
}
impl Layout {
    pub fn new(content: &Content) -> Self {
        let mut result = Self {
            nodes: vec![V2::ZERO; content.classes.len()],
            ..Self::default()
        };
        let tiers = content.classes.iter().map(|c| c.tier).max().unwrap_or(0);
        let mut top = 0.;
        for tier in 0..=tiers {
            let mut row: Vec<_> = content
                .classes
                .iter()
                .enumerate()
                .filter_map(|(i, c)| (c.tier == tier).then_some(i))
                .collect();
            // Keep sibling branches together and use names as a stable tie-break.
            let center = |index: usize| {
                let parents = &content.classes[index].parents;
                let positions: Vec<_> = parents
                    .iter()
                    .filter_map(|id| content.classes.iter().position(|c| &c.id == id))
                    .map(|i| result.nodes[i].x)
                    .collect();
                positions.iter().sum::<f32>() / positions.len().max(1) as f32
            };
            row.sort_by(|&a, &b| {
                center(a)
                    .total_cmp(&center(b))
                    .then_with(|| content.classes[a].name.cmp(&content.classes[b].name))
            });
            let columns = row.len().clamp(1, 28);
            result.width = result.width.max(columns as f32 * 124.);
            for (slot, index) in row.iter().enumerate() {
                result.nodes[*index] = V2::new(
                    ((slot % columns) as f32 - columns.saturating_sub(1) as f32 * 0.5) * 124.,
                    top + (slot / columns) as f32 * 100.,
                );
            }
            top += row.len().div_ceil(columns).saturating_sub(1) as f32 * 100. + 172.;
        }
        for (child, class) in content.classes.iter().enumerate() {
            for parent in &class.parents {
                if let Some(parent) = content.classes.iter().position(|c| &c.id == parent) {
                    result.edges.push((parent, child));
                }
            }
        }
        result.height = (top - 172.).max(0.) + 100.;
        result
    }
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
    }
}
