enum Tree {
    Leaf,
    InnerNode(Vec<Tree>),
}

fn total_nodes(tree: &Tree) -> usize {
    match tree {
        Tree::Leaf => 1,
        Tree::InnerNode(children) => {
            let mut result = 1;
            for child in children {
                result += total_nodes(child);
            }
            result
        }
    }
}

#[test]
fn test_dfs() {
    let mut tree = Tree::Leaf;
    const N: usize = 1_000_000;
    for _ in 0..N {
        tree = Tree::InnerNode(vec![tree, Tree::Leaf]);
    }
    assert_eq!(total_nodes(&tree), N * 2 + 1);
}
