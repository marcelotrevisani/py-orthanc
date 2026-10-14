use std::collections::{HashMap, HashSet, VecDeque};

use iced::mouse;
use iced::widget::{canvas, column, container, row, text};
use iced::{Element, Length, Point, Rectangle, Size, Theme, Vector};

use super::theme;
use crate::app::{DepTreeNode, Message};

// ── Graph layout data ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct GraphNode {
    name: String,
    version: String,
    x: f32,
    y: f32,
    depth: usize,
    is_root: bool,
}

#[derive(Debug, Clone)]
struct GraphEdge {
    from: usize,
    to: usize,
}

fn layout_graph(tree: &[DepTreeNode]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut name_to_idx: HashMap<String, usize> = HashMap::new();

    // Each package becomes one node; repeated or cyclic references only add an edge.
    fn collect(
        node: &DepTreeNode,
        depth: usize,
        parent_idx: Option<usize>,
        nodes: &mut Vec<GraphNode>,
        edges: &mut Vec<GraphEdge>,
        name_to_idx: &mut HashMap<String, usize>,
    ) {
        let key = node.name.to_lowercase();
        let existing = name_to_idx.get(&key).copied();
        let idx = existing.unwrap_or(nodes.len());
        if let Some(parent) = parent_idx {
            edges.push(GraphEdge {
                from: parent,
                to: idx,
            });
        }
        if existing.is_some() {
            return;
        }

        nodes.push(GraphNode {
            name: node.name.clone(),
            version: node.version.clone(),
            x: 0.0, // will be calculated after
            y: 0.0,
            depth,
            is_root: parent_idx.is_none(),
        });
        name_to_idx.insert(key, idx);

        for child in &node.children {
            collect(child, depth + 1, Some(idx), nodes, edges, name_to_idx);
        }
    }

    for root in tree {
        collect(root, 0, None, &mut nodes, &mut edges, &mut name_to_idx);
    }

    // Layout: horizontal layers by depth
    let h_spacing = 220.0_f32;
    let v_spacing = 60.0_f32;
    let mut depth_positions: HashMap<usize, usize> = HashMap::new();

    for node in &mut nodes {
        let pos = depth_positions.entry(node.depth).or_insert(0);
        node.x = node.depth as f32 * h_spacing + 100.0;
        node.y = *pos as f32 * v_spacing + 80.0;
        *pos += 1;
    }

    (nodes, edges)
}

/// Compute the full set of transitively connected nodes from a starting node.
/// Follows edges in both directions (ancestors and descendants).
fn connected_set(start: usize, edges: &[GraphEdge], node_count: usize) -> HashSet<usize> {
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for edge in edges {
        adjacency[edge.from].push(edge.to);
        adjacency[edge.to].push(edge.from);
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some(node) = queue.pop_front() {
        for &neighbor in &adjacency[node] {
            if visited.insert(neighbor) {
                queue.push_back(neighbor);
            }
        }
    }

    visited
}

/// Compute the subgraph reachable from a node following directed edges
/// (both downstream children and upstream parents).
fn directed_subgraph(start: usize, edges: &[GraphEdge], node_count: usize) -> HashSet<usize> {
    // Build forward and reverse adjacency
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    let mut parents: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for edge in edges {
        children[edge.from].push(edge.to);
        parents[edge.to].push(edge.from);
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Collect all descendants
    queue.push_back(start);
    visited.insert(start);
    while let Some(node) = queue.pop_front() {
        for &child in &children[node] {
            if visited.insert(child) {
                queue.push_back(child);
            }
        }
    }

    // Collect all ancestors
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        for &parent in &parents[node] {
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    visited
}

// ── Canvas program ───────────────────────────────────────────────────────

pub struct DepGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    cache: canvas::Cache,
}

#[derive(Default)]
pub struct GraphState {
    pan: Vector,
    zoom: f32,
    dragging: bool,
    last_cursor: Option<Point>,
    hovered_node: Option<usize>,
    /// When set, only show the subgraph related to this node
    filtered_node: Option<usize>,
    /// Timestamp of last click for double-click detection
    last_click_time: Option<std::time::Instant>,
    /// Node index of last click for double-click detection
    last_click_node: Option<usize>,
}

impl DepGraph {
    fn new(tree: &[DepTreeNode]) -> Self {
        let (nodes, edges) = layout_graph(tree);
        Self {
            nodes,
            edges,
            cache: canvas::Cache::new(),
        }
    }

    /// Hit-test: find which node (if any) is at the given canvas position
    fn hit_test(&self, pos: Point, zoom: f32, pan: Vector) -> Option<usize> {
        for (i, node) in self.nodes.iter().enumerate() {
            let nx = node.x * zoom + pan.x;
            let ny = node.y * zoom + pan.y;
            if pos.x >= nx - 5.0
                && pos.x <= nx + NODE_W * zoom + 5.0
                && pos.y >= ny - 5.0
                && pos.y <= ny + NODE_H * zoom + 5.0
            {
                return Some(i);
            }
        }
        None
    }
}

const NODE_W: f32 = 160.0;
const NODE_H: f32 = 36.0;
const DOUBLE_CLICK_MS: u128 = 400;

impl canvas::Program<Message> for DepGraph {
    type State = GraphState;

    fn update(
        &self,
        state: &mut GraphState,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Option<Message>) {
        let zoom = if state.zoom == 0.0 { 1.0 } else { state.zoom };

        match event {
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / 50.0,
                };
                let factor = 1.0 + dy * 0.1;
                state.zoom = (zoom * factor).clamp(0.1, 5.0);
                self.cache.clear();
                (iced::event::Status::Captured, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    // Check for double-click on a node
                    if let Some(pos) = cursor.position_in(bounds) {
                        if let Some(clicked_node) = self.hit_test(pos, zoom, state.pan) {
                            let now = std::time::Instant::now();
                            let is_double_click = state.last_click_node == Some(clicked_node)
                                && state.last_click_time.is_some_and(|t| {
                                    now.duration_since(t).as_millis() < DOUBLE_CLICK_MS
                                });

                            if is_double_click {
                                // Toggle filter: if already filtered to this node, clear it
                                if state.filtered_node == Some(clicked_node) {
                                    state.filtered_node = None;
                                } else {
                                    state.filtered_node = Some(clicked_node);
                                }
                                state.last_click_node = None;
                                state.last_click_time = None;
                                self.cache.clear();
                                return (iced::event::Status::Captured, None);
                            }

                            state.last_click_node = Some(clicked_node);
                            state.last_click_time = Some(now);
                        } else {
                            state.last_click_node = None;
                            state.last_click_time = None;
                        }
                    }

                    state.dragging = true;
                    state.last_cursor = cursor.position();
                    (iced::event::Status::Captured, None)
                } else {
                    (iced::event::Status::Ignored, None)
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
                state.last_cursor = None;
                (iced::event::Status::Captured, None)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.dragging {
                    if let Some(last) = state.last_cursor {
                        let dx = position.x - last.x;
                        let dy = position.y - last.y;
                        state.pan = state.pan + Vector::new(dx, dy);
                        self.cache.clear();
                    }
                    state.last_cursor = Some(position);
                    (iced::event::Status::Captured, None)
                } else {
                    // Hit-test for hover
                    let old_hover = state.hovered_node;
                    state.hovered_node = None;
                    if let Some(pos) = cursor.position_in(bounds) {
                        state.hovered_node = self.hit_test(pos, zoom, state.pan);
                    }
                    if old_hover != state.hovered_node {
                        self.cache.clear();
                    }
                    (iced::event::Status::Ignored, None)
                }
            }
            _ => (iced::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let content = self.cache.draw(renderer, bounds.size(), |frame| {
            let zoom = if state.zoom == 0.0 { 1.0 } else { state.zoom };
            let pan = state.pan;

            // Compute the highlight set (all transitively connected nodes)
            let highlight_set: Option<HashSet<usize>> = state
                .hovered_node
                .map(|h| connected_set(h, &self.edges, self.nodes.len()));

            // Compute the filter set (directed subgraph for double-clicked node)
            let filter_set: Option<HashSet<usize>> = state
                .filtered_node
                .map(|f| directed_subgraph(f, &self.edges, self.nodes.len()));

            // Draw edges
            for edge in &self.edges {
                // Skip edges not in filtered subgraph
                if let Some(ref fs) = filter_set {
                    if !fs.contains(&edge.from) || !fs.contains(&edge.to) {
                        continue;
                    }
                }

                let from = &self.nodes[edge.from];
                let to = &self.nodes[edge.to];

                let x1 = from.x * zoom + pan.x + NODE_W * zoom;
                let y1 = from.y * zoom + pan.y + NODE_H * zoom * 0.5;
                let x2 = to.x * zoom + pan.x;
                let y2 = to.y * zoom + pan.y + NODE_H * zoom * 0.5;

                let mid_x = (x1 + x2) * 0.5;

                let is_highlight = highlight_set
                    .as_ref()
                    .is_some_and(|hs| hs.contains(&edge.from) && hs.contains(&edge.to));

                let edge_color = if is_highlight {
                    theme::ACCENT_LIGHT()
                } else {
                    theme::TEXT_MUTED().scale_alpha(0.4)
                };

                let path = canvas::path::Path::new(|b| {
                    b.move_to(Point::new(x1, y1));
                    b.bezier_curve_to(
                        Point::new(mid_x, y1),
                        Point::new(mid_x, y2),
                        Point::new(x2, y2),
                    );
                });
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(edge_color)
                        .with_width(if is_highlight { 2.0 } else { 1.0 }),
                );
            }

            // Draw nodes
            for (i, node) in self.nodes.iter().enumerate() {
                // Skip nodes not in filtered subgraph
                if let Some(ref fs) = filter_set {
                    if !fs.contains(&i) {
                        continue;
                    }
                }

                let x = node.x * zoom + pan.x;
                let y = node.y * zoom + pan.y;
                let w = NODE_W * zoom;
                let h = NODE_H * zoom;

                let is_hovered = state.hovered_node == Some(i);
                let is_in_highlight = highlight_set.as_ref().is_some_and(|hs| hs.contains(&i));
                let is_filtered_root = state.filtered_node == Some(i);
                let has_hover_active = state.hovered_node.is_some();

                let (bg, border_color) = if is_filtered_root {
                    // The double-clicked focus node gets a strong accent
                    (theme::ACCENT().scale_alpha(0.4), theme::ACCENT_LIGHT())
                } else if is_hovered {
                    (theme::BG_MID(), theme::ACCENT_LIGHT())
                } else if is_in_highlight {
                    // Part of the connected set
                    (
                        theme::ACCENT().scale_alpha(0.15),
                        theme::ACCENT_LIGHT().scale_alpha(0.7),
                    )
                } else if has_hover_active {
                    // Not connected — dim it
                    (
                        theme::BG_DARK().scale_alpha(0.4),
                        theme::BORDER().scale_alpha(0.3),
                    )
                } else if node.is_root {
                    (theme::ACCENT().scale_alpha(0.2), theme::ACCENT_LIGHT())
                } else {
                    (theme::BG_DARK(), theme::BORDER())
                };

                let rect = canvas::path::Path::rounded_rectangle(
                    Point::new(x, y),
                    Size::new(w, h),
                    (4.0 * zoom).into(),
                );
                frame.fill(&rect, bg);
                frame.stroke(
                    &rect,
                    canvas::Stroke::default()
                        .with_color(border_color)
                        .with_width(if is_in_highlight || is_filtered_root {
                            1.5
                        } else {
                            1.0
                        }),
                );

                // Text alpha for dimmed nodes
                let text_alpha = if has_hover_active && !is_in_highlight {
                    0.3
                } else {
                    1.0
                };

                // Name
                let name_size = (12.0 * zoom).max(6.0);
                let name_color = if is_filtered_root || node.is_root {
                    theme::ACCENT_LIGHT().scale_alpha(text_alpha)
                } else {
                    theme::TEXT_PRIMARY().scale_alpha(text_alpha)
                };
                frame.fill_text(canvas::Text {
                    content: node.name.clone(),
                    position: Point::new(x + 8.0 * zoom, y + 6.0 * zoom),
                    color: name_color,
                    size: name_size.into(),
                    ..canvas::Text::default()
                });

                // Version
                if zoom > 0.4 {
                    let ver_size = (10.0 * zoom).max(5.0);
                    frame.fill_text(canvas::Text {
                        content: node.version.clone(),
                        position: Point::new(x + 8.0 * zoom, y + 22.0 * zoom),
                        color: theme::TEXT_MUTED().scale_alpha(text_alpha),
                        size: ver_size.into(),
                        ..canvas::Text::default()
                    });
                }
            }
        });

        vec![content]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if state.hovered_node.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

// ── Public view functions ────────────────────────────────────────────────

/// View for the graph window (separate window)
pub fn view_graph<'a>(tree: Option<&[DepTreeNode]>, loading: bool) -> Element<'a, Message> {
    let Some(tree) = tree.filter(|_| !loading) else {
        return container(
            column![
                text("Loading dependency graph...")
                    .size(16)
                    .color(theme::TEXT_MUTED()),
                text("Fetching package metadata...")
                    .size(12)
                    .color(theme::TEXT_MUTED()),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(40)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(theme::BG_DARKEST())),
            ..Default::default()
        })
        .into();
    };

    let graph = DepGraph::new(tree);

    let hint = text(
        "Scroll to zoom | Drag to pan | Double-click node to filter | Double-click again to reset",
    )
    .size(11)
    .color(theme::TEXT_MUTED());

    let total = count_unique(tree);
    let stats = text(format!("{} roots, {} total packages", tree.len(), total))
        .size(12)
        .color(theme::TEXT_SECONDARY());

    container(
        column![
            row![stats, iced::widget::horizontal_space(), hint]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .padding([8, 16]),
            canvas(graph).width(Length::Fill).height(Length::Fill),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| container::Style {
        background: Some(iced::Background::Color(theme::BG_DARKEST())),
        ..Default::default()
    })
    .into()
}

fn count_unique(nodes: &[DepTreeNode]) -> usize {
    let mut set = HashSet::new();
    fn collect(node: &DepTreeNode, set: &mut HashSet<String>) {
        set.insert(node.name.to_lowercase());
        for child in &node.children {
            collect(child, set);
        }
    }
    for node in nodes {
        collect(node, &mut set);
    }
    set.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(name: &str, children: Vec<DepTreeNode>) -> DepTreeNode {
        DepTreeNode {
            name: name.to_string(),
            version: "1".to_string(),
            children,
        }
    }

    #[test]
    fn layout_graph_dedupes_shared_and_cyclic_deps() {
        // a -> {b, c}, b -> c, c -> A (case-insensitive cycle back to the root)
        let tree = vec![dep(
            "a",
            vec![
                dep("b", vec![dep("c", vec![dep("A", vec![])])]),
                dep("c", vec![]),
            ],
        )];
        let (nodes, edges) = layout_graph(&tree);

        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert!(nodes[0].is_root && !nodes[1].is_root);
        assert_eq!(nodes.iter().map(|n| n.depth).collect::<Vec<_>>(), [0, 1, 2]);
        let pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
        assert_eq!(pairs, [(0, 1), (1, 2), (2, 0), (0, 2)]);
    }
}
