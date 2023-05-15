use crate::graph::*;
use std::rc::Rc;
use yew::prelude::*;

#[derive(PartialEq, Properties)]
pub struct GraphCanvasProperties {
    #[prop_or_default]
    pub id: AttrValue,

    #[prop_or_default("width: 100%; height: 100%;")]
    pub style: AttrValue,
    #[prop_or_default]
    pub class: Classes,

    pub initializer: GraphInitializer,
}

#[derive(Clone)]
pub struct GraphInitializer(pub Rc<dyn Fn(&mut Graph)>);

impl GraphInitializer {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&mut Graph) + 'static,
    {
        Self(Rc::new(f))
    }
}

impl PartialEq for GraphInitializer {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[function_component(GraphCanvas)]
pub fn graph_canvas(props: &GraphCanvasProperties) -> Html {
    let canvas = use_node_ref();

    {
        let canvas = canvas.clone();
        use_effect_with_deps(
            move |initializer| {
                let mut graph = Graph::new(canvas.cast().unwrap());

                initializer.0(&mut graph);

                let handle = graph.run();

                || {
                    log::debug!("Dropping graph");
                    drop(handle);
                }
            },
            props.initializer.clone(),
        );
    }

    html!(
        <canvas
            id={&props.id}
            ref={canvas}
            class={props.class.clone()}
            style={&props.style}
        >
        </canvas>
    )
}
