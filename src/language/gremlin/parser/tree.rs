//! Heap-based parse-tree formatting and destruction. ANTLR's default recursive
//! walkers can overflow even after parsing and lowering successfully grew stacks.
use super::g::GremlinParserContext;
use std::rc::Rc;

type Node<'a> = Rc<dyn GremlinParserContext<'a> + 'a>;
pub(super) struct TreeOwner<'a>(pub Node<'a>);
impl Drop for TreeOwner<'_> {
    fn drop(&mut self) {
        let mut pending = vec![self.0.clone()];
        while let Some(node) = pending.pop() {
            pending.extend(node.get_children());
            // Retain children in the worklist before releasing parent ownership.
            for _ in 0..node.get_child_count() {
                node.remove_last_child();
            }
        }
    }
}

pub(super) fn format_tree<'a>(root: Node<'a>, names: &[&str]) -> String {
    enum Part<'a> {
        Node(Node<'a>),
        Space,
        Close,
    }
    let mut pending = vec![Part::Node(root)];
    let mut output = String::new();
    while let Some(part) = pending.pop() {
        match part {
            Part::Space => output.push(' '),
            Part::Close => output.push(')'),
            Part::Node(node) => {
                let children = node.get_children().collect::<Vec<_>>();
                if !children.is_empty() {
                    output.push('(');
                }
                for ch in node.get_node_text(names).chars() {
                    match ch {
                        '\t' => output.push_str("\\t"),
                        '\n' => output.push_str("\\n"),
                        '\r' => output.push_str("\\r"),
                        _ => output.push(ch),
                    }
                }
                if !children.is_empty() {
                    pending.push(Part::Close);
                    for child in children.into_iter().rev() {
                        pending.push(Part::Node(child));
                        pending.push(Part::Space);
                    }
                }
            }
        }
    }
    output
}
