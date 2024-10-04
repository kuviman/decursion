use decursion::*;

struct Children(Vec<Tree>);

impl From<Vec<Tree>> for Children {
    fn from(value: Vec<Tree>) -> Self {
        Self(value)
    }
}

impl Drop for Children {
    fn drop(&mut self) {
        std::mem::forget(std::mem::take(&mut self.0)); // TODO dont leak
    }
}

impl IntoIterator for Children {
    type Item = Tree;
    type IntoIter = Box<dyn Iterator<Item = Tree>>;
    fn into_iter(mut self) -> Self::IntoIter {
        Box::new(std::mem::take(&mut self.0).into_iter())
    }
}

enum Tree {
    Leaf,
    InnerNode(Children),
}

async fn total_nodes(tree: Tree) -> usize {
    match tree {
        Tree::Leaf => 1,
        Tree::InnerNode(children) => {
            let mut result = 1;
            for child in children {
                result += total_nodes(child).decurse().await;
            }
            result
        }
    }
}

fn broken_total_nodes(tree: Tree) -> usize {
    match tree {
        Tree::Leaf => 1,
        Tree::InnerNode(children) => {
            let mut result = 1;
            for child in children {
                result += broken_total_nodes(child);
            }
            result
        }
    }
}

#[test]
#[ignore = "this test stack overflows"]
fn test_broken() {
    let mut tree = Tree::Leaf;
    const N: usize = 1_000_000;
    for _ in 0..N {
        tree = Tree::InnerNode(vec![tree, Tree::Leaf].into());
    }
    assert_eq!(broken_total_nodes(tree), N * 2 + 1);
}

#[test]
fn test_decursed() {
    let start = std::time::Instant::now();
    let mut tree = Tree::Leaf;
    const N: usize = 1_000_000;
    for _ in 0..N {
        tree = Tree::InnerNode(vec![tree, Tree::Leaf].into());
    }
    println!("constructed tree in {:?}", start.elapsed());
    let start = std::time::Instant::now();
    assert_eq!(run_decursing(total_nodes(tree)), N * 2 + 1);
    println!("finished in {:?}", start.elapsed());
}
