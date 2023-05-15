use js_sys::Math::{cos, random};
use web_graph::component::GraphInitializer;
use web_graph::{
    component::GraphCanvas,
    graph::{EdgeProperties, NodeProperties},
};
use yew::prelude::*;

#[function_component(Application)]
pub fn app() -> Html {
    html!(
        <GraphCanvas
            style="width: 100%; height: 100%;"
            initializer={GraphInitializer::new(|graph| {

                let mut nodes = Vec::new();

                const NUM: usize = 10;
                for _ in 0..NUM {
                    let x = random() * 1900.0;
                    let y = random() * 1000.0;
                    nodes.push(graph.add_node(
                        (x, y),
                        (50.0, 50.0),
                        NodeProperties {
                            label: "Foo".to_string(),
                        },
                    ));
                }

                for _ in 0..5 {
                    let a = (cos(random()) * (NUM as f64)) as usize;
                    let b = (random() * (NUM as f64)) as usize;

                    let a = nodes.get(a);
                    let b = nodes.get(b);
                    if let Some((a, b)) = a.zip(b) {
                        graph.add_edge(
                            *a,
                            *b,
                            EdgeProperties {
                                weight: (100.0 + random() * 500.0) as _,
                            },
                        );
                    }
                }

            })}
        />
    )
}
