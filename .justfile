default:
  just --list

overflow:
  cargo test -- --nocapture --exact test_overflow --include-ignored
