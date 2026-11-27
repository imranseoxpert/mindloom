# Glossary (JS name → Rust name)

| Term | Meaning | Rust |
| --- | --- | --- |
| doc | A whole map: root, floating, relations, settings | `model::Doc` |
| node / topic | One box on the map | `model::Node` |
| root / central topic | The map's main node | `Doc::root` |
| floating topic | A tree not attached to the root, at `x`,`y` | `Doc::floating` |
| relation | A labelled dashed arrow between two topics | `model::Relation` |
| boundary | Dashed box around a topic's branch | `Node::boundary` |
| offset | Hand-placed shift `{x, y, l}`; `l` = layout it was made in | `model::Offset` |
| side | Mind-map side of a main topic (`left`/`right`) | `Node::side` |
| settings | Map-wide theme/layout/line/pattern/font/spacing | `model::Settings` |
| VT / visual tree | A laid-out node with box and position | `layout::Item` |
| box | A node's measured inner geometry | `layout::NodeBox` |
| edge | Connector between parent and child | `layout::Edge` |
| measure | Text width function (injected) | `layout::Measure` |
