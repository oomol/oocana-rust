# Vocana Rust

![main](https://github.com/oomol/vocana-rust/actions/workflows/build-and-test.yml/badge.svg?branch=main)

## Demo

```bash
cargo run run examples
```

## Develop

clean:
	cargo clean
	find . -type f -name "*.orig" -exec rm {} \;
	find . -type f -name "*.bk" -exec rm {} \;
	find . -type f -name ".*~" -exec rm {} \;
