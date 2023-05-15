use gloo_events::EventListener;
use js_sys::{
    Math::{abs, atan2, cos, max, min, pow, sin, sqrt},
    Object,
};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::mem::swap;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, EventTarget, HtmlCanvasElement, MouseEvent};

const MAX_MOVE: f64 = 5.0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("web error: {0}")]
    Web(String),
    #[error("missing canvas context")]
    MissingContext,
}

impl From<JsValue> for Error {
    fn from(value: JsValue) -> Self {
        if let Some(value) = value.as_string() {
            Self::Web(value)
        } else {
            Self::Web(format!("{:?}", value))
        }
    }
}

impl From<Object> for Error {
    fn from(value: Object) -> Self {
        Self::Web(format!("{:?}", value))
    }
}

pub struct Graph {
    canvas: HtmlCanvasElement,
    counter: usize,
    nodes: HashMap<Node, Rc<RefCell<NodeState>>>,

    edges: HashMap<Node, HashMap<Node, Rc<EdgeState>>>,
    edges_rev: HashMap<Node, HashMap<Node, Rc<EdgeState>>>,

    hovering: Option<Node>,
    dragging: bool,
}

impl Graph {
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        let result = Self {
            canvas,
            counter: 0,
            nodes: Default::default(),
            edges: Default::default(),
            edges_rev: Default::default(),
            hovering: None,
            dragging: false,
        };

        result.adjust_resolution();

        result
    }

    pub fn adjust_resolution(&self) {
        fn get_style_dimensions(graph: &Graph) -> Option<(f64, f64)> {
            let window = gloo_utils::window();

            if let Ok(Some(style)) = window.get_computed_style(&graph.canvas) {
                if let (Ok(width), Ok(height)) = (
                    style.get_property_value("width"),
                    style.get_property_value("height"),
                ) {
                    if let (Some(width), Some(height)) =
                        (width.strip_suffix("px"), height.strip_suffix("px"))
                    {
                        if let (Ok(width), Ok(height)) = (width.parse(), height.parse()) {
                            return Some((width, height));
                        }
                    }
                }
            }

            None
        }

        if let Some((width, height)) = get_style_dimensions(self) {
            let window = gloo_utils::window();
            let dpi = window.device_pixel_ratio();

            let width = (width * dpi) as u32;
            let height = (height * dpi) as u32;

            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }
    }

    pub fn add_node(
        &mut self,
        position: impl Into<Position>,
        size: impl Into<Size>,
        node: NodeProperties,
    ) -> Node {
        let handle = Node { id: self.counter };
        self.counter += 1;

        let state = NodeState {
            properties: node,
            handle,
            position: position.into(),
            size: size.into(),
        };

        self.nodes.insert(handle, Rc::new(RefCell::new(state)));

        handle
    }

    pub fn remove_node(&mut self, node: Node) {}

    pub fn add_edge(&mut self, mut a: Node, mut b: Node, edge: EdgeProperties) {
        let state = Rc::new(EdgeState { properties: edge });

        match a.cmp(&b) {
            Ordering::Equal => return,
            Ordering::Less => {}
            Ordering::Greater => {
                // ensure that the smaller one is "a", so that we don't create duplicate entries
                swap(&mut a, &mut b);
            }
        }

        // we add them twice, in both directions
        self.edges.entry(a).or_default().insert(b, state.clone());
        self.edges_rev.entry(b).or_default().insert(a, state);
    }

    pub fn remove_edge(&mut self, edge: Edge) {}

    pub fn draw(&self) -> Result<(), Error> {
        let ctx = self
            .canvas
            .get_context("2d")?
            .ok_or_else(|| Error::MissingContext)?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

        // self.adjust_resolution();

        ctx.clear_rect(
            0.0,
            0.0,
            self.canvas.width() as _,
            self.canvas.height() as _,
        );

        let dpi = gloo_utils::window().device_pixel_ratio();

        ctx.save();
        let _ = ctx.scale(dpi, dpi);

        // draw edges first

        for (from, edges) in &self.edges {
            // we can do better here, instead of doing another lookup and unwrapping, we should
            // find a way to keep a reference to the nodes (from and to).
            let from = self.nodes.get(from).unwrap();
            for (to, _edge) in edges {
                let to = self.nodes.get(to).unwrap();
                ctx.begin_path();

                let Position { x, y } = from.borrow().center();
                ctx.move_to(x, y);

                let Position { x, y } = to.borrow().center();
                ctx.line_to(x, y);

                ctx.stroke();
            }
        }

        // next draw nodes

        ctx.set_fill_style(&JsValue::from_str("red"));
        for (id, node) in &self.nodes {
            let node = node.borrow();

            ctx.begin_path();
            ctx.fill_rect(
                node.position.x,
                node.position.y,
                node.size.width,
                node.size.height,
            );
            if self.hovering == Some(*id) {
                ctx.set_line_width(5.0);
            } else {
                ctx.set_line_width(1.0);
            }
            ctx.rect(
                node.position.x,
                node.position.y,
                node.size.width,
                node.size.height,
            );
            ctx.stroke();
        }

        ctx.restore();

        Ok(())
    }

    fn tick(&mut self) {
        self.walk_edges();
        // self.walk_all_nodes();
    }

    fn walk_all_nodes(&mut self) {
        for (from, from_state) in &self.nodes {
            for (to, to_state) in &self.nodes {
                if from == to {
                    continue;
                }

                let distance = abs(from_state
                    .borrow()
                    .position
                    .delta(to_state.borrow().position));

                if distance < 100.0 {
                    // let's move away from it
                    let delta = -100.0 - distance;
                    if !self.dragging || self.hovering != Some(*from) {
                        from_state
                            .borrow_mut()
                            .move_to(delta / 2.0, to_state.borrow().center());
                    }
                    if !self.dragging || self.hovering != Some(*to) {
                        to_state
                            .borrow_mut()
                            .move_to(delta / 2.0, from_state.borrow().center());
                    }
                }
            }
        }
    }

    fn walk_edges(&mut self) {
        for (from, edges) in &self.edges {
            // again, I think we can do better here
            let from_state = self.nodes.get(from).unwrap();
            for (to, edge) in edges {
                let to_state = self.nodes.get(to).unwrap();

                let distance = abs(from_state
                    .borrow()
                    .position
                    .delta(to_state.borrow().position));

                // the delta we want to move
                let delta = distance - edge.properties.weight as f64;
                if abs(delta) > 0.1 {
                    // move only if we don't drag them
                    if !self.dragging || self.hovering != Some(*from) {
                        from_state
                            .borrow_mut()
                            .move_to(delta / 2.0, to_state.borrow().center());
                    }
                    if !self.dragging || self.hovering != Some(*to) {
                        to_state
                            .borrow_mut()
                            .move_to(delta / 2.0, from_state.borrow().center());
                    }
                }
            }
        }
    }

    pub fn run(self) -> Handle {
        fn request_animation_frame(f: &Closure<dyn FnMut()>) {
            gloo_utils::window()
                .request_animation_frame(f.as_ref().unchecked_ref())
                .expect("should register `requestAnimationFrame` OK");
        }

        let canvas = self.canvas.clone();
        let graph = Rc::new(RefCell::new(self));

        fn mouse_event<F>(
            target: &EventTarget,
            event_type: &'static str,
            graph: &Rc<RefCell<Graph>>,
            f: F,
        ) -> EventListener
        where
            F: Fn(&mut Graph, &MouseEvent) + 'static,
        {
            let graph = graph.clone();
            EventListener::new(target, event_type, move |evt| {
                if let Ok(mut graph) = graph.try_borrow_mut() {
                    if let Some(evt) = evt.dyn_ref::<MouseEvent>() {
                        f(&mut graph, evt);
                    }
                }
            })
        }

        let mut listeners = vec![];

        listeners.push(mouse_event(&canvas, "mousedown", &graph, |graph, evt| {
            graph.mouse_down(evt);
        }));

        listeners.push(mouse_event(&canvas, "mouseup", &graph, |graph, evt| {
            graph.mouse_up(evt);
        }));

        listeners.push(mouse_event(&canvas, "mouseout", &graph, |graph, evt| {
            graph.mouse_out(evt);
        }));

        {
            let graph = graph.clone();
            listeners.push(EventListener::new(&canvas, "mousemove", move |evt| {
                if let Ok(mut graph) = graph.try_borrow_mut() {
                    if let Some(evt) = evt.dyn_ref::<MouseEvent>() {
                        graph.mouse_move(evt);
                    }
                }
            }))
        }

        let f = Rc::new(RefCell::new(None));
        let g = f.clone();

        *g.borrow_mut() = Some(Closure::new(move || {
            if let Ok(mut graph) = graph.try_borrow_mut() {
                graph.tick();
                let _ = graph.draw();
            }

            request_animation_frame(f.borrow().as_ref().unwrap());
        }));

        request_animation_frame(g.clone().borrow().as_ref().unwrap());

        Handle {
            _render_loop: g,
            listeners,
        }
    }

    fn mouse_move(&mut self, evt: &MouseEvent) {
        //log::info!("Move: {}", Position::from(evt));
        let position = self.adjust_mouse_position(evt.into());

        if let Some(selected) = self.hovering.and_then(|n| self.nodes.get_mut(&n)) {
            if self.dragging {
                // if we are dragging, we don't lose the selection
                selected.borrow_mut().set_centered(position);
            } else if !selected.borrow().contains(position) {
                // lost selection
                self.hovering = None;
            }
        }

        // try selecting a new none
        if self.hovering.is_none() {
            self.hovering = self.first_node(position).map(|(id, _)| id.clone());
        }
    }

    fn mouse_down(&mut self, _evt: &MouseEvent) {
        self.dragging = self.hovering.is_some();
    }

    fn mouse_up(&mut self, _evt: &MouseEvent) {
        self.dragging = false;
    }

    fn mouse_out(&mut self, _evt: &MouseEvent) {
        self.dragging = false;
        self.hovering = None;
    }

    fn first_node(
        &self,
        position: impl Into<Position>,
    ) -> Option<(&Node, &Rc<RefCell<NodeState>>)> {
        let position = position.into();
        self.nodes
            .iter()
            .find(|(_, n)| n.borrow().contains(position))
    }

    fn adjust_mouse_position(&self, position: Position) -> Position {
        let rect = self.canvas.get_bounding_client_rect();
        Position {
            x: position.x - rect.left(),
            y: position.y - rect.top(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl Position {
    pub fn delta(&self, other: Position) -> f64 {
        sqrt(pow(other.x - self.x, 2.0) + pow(other.y - self.y, 2.0))
    }
}

impl Display for Position {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.x, self.y)
    }
}

impl From<(f64, f64)> for Position {
    fn from((x, y): (f64, f64)) -> Self {
        Self { x, y }
    }
}

impl From<&MouseEvent> for Position {
    fn from(value: &MouseEvent) -> Self {
        Self {
            x: value.client_x() as _,
            y: value.client_y() as _,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl From<(f64, f64)> for Size {
    fn from((width, height): (f64, f64)) -> Self {
        Self { width, height }
    }
}

pub struct Handle {
    _render_loop: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    listeners: Vec<EventListener>,
}

struct EdgeState {
    properties: EdgeProperties,
}

struct NodeState {
    properties: NodeProperties,
    handle: Node,
    position: Position,
    size: Size,
}

impl NodeState {
    fn contains(&self, position: impl Into<Position>) -> bool {
        let position = position.into();
        position.x >= self.position.x
            && position.y >= self.position.y
            && position.x <= (self.position.x + self.size.width)
            && position.y <= (self.position.y + self.size.height)
    }

    fn set_centered(&mut self, position: impl Into<Position>) {
        let position = position.into();
        self.position = Position {
            x: position.x - self.size.width / 2.0,
            y: position.y - self.size.height / 2.0,
        }
    }

    fn center(&self) -> Position {
        Position {
            x: self.position.x + self.size.width / 2.0,
            y: self.position.y + self.size.height / 2.0,
        }
    }

    fn move_to(&mut self, amount: f64, position: Position) {
        let amount = max(min(amount, MAX_MOVE), -MAX_MOVE);
        let angle = atan2(position.y - self.position.y, position.x - self.position.x);
        self.position.x += cos(angle) * amount;
        self.position.y += sin(angle) * amount;
    }
}

pub struct EdgeProperties {
    pub weight: usize,
}

pub struct NodeProperties {
    pub label: String,
}

pub struct Edge {}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Node {
    id: usize,
}
