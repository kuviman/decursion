#include <coroutine>
#include <iostream>
#include <vector>

using namespace std;

struct Tree {
  vector<Tree> children;
};

Tree* constructTree() {
  Tree* tree = new Tree { .children = vector<Tree>() };
  Tree* current = tree;
  for (int i = 0; i < 1'000'000; i++) {
    current->children.push_back({ .children = vector<Tree>() });
    current = &current->children[0];
  }
  return tree;
}

int calculateNodes(Tree* tree) {
  int result = 1;
  for (auto it = tree->children.begin(); it != tree->children.end(); it++) {
    result += calculateNodes(&*it);
  }
  return result;
}

int main() {
  Tree* tree = constructTree();
  cout << "tree constructed" << endl;
  int result = calculateNodes(tree);
  cout << "result = " << result << endl;
  return 0;
}
