use decursion::*;

#[derive(Default)]
struct Tree {
    children: Vec<Tree>,
}

impl Drop for Tree {
    fn drop(&mut self) {
        if self.children.is_empty() {
            return;
        }
        async fn safe_drop(mut tree: Tree) {
            for child in std::mem::take(&mut tree.children) {
                safe_drop(child).decurse().await;
            }
        }
        run_decursing(safe_drop(std::mem::take(self)));
    }
}

async fn total_nodes(mut tree: Tree) -> usize {
    let mut result = 1;
    for child in std::mem::take(&mut tree.children) {
        result += total_nodes(child).decurse().await;
    }
    result
}

fn recursive_total_nodes(mut tree: Tree) -> usize {
    let mut result = 1;
    for child in std::mem::take(&mut tree.children) {
        result += recursive_total_nodes(child);
    }
    result
}

const N: usize = 1_000_000;

fn construct_tree() -> Tree {
    let mut tree = Tree::default();
    for _ in 0..N {
        tree = Tree {
            children: vec![tree, Tree::default()],
        };
    }
    tree
}

#[test]
#[ignore = "this test stack overflows"]
fn test_overflow() {
    let tree = construct_tree();
    assert_eq!(recursive_total_nodes(tree), N * 2 + 1);
}

#[test]
fn test_decursed() {
    let tree = construct_tree();
    assert_eq!(run_decursing(total_nodes(tree)), N * 2 + 1);
}
