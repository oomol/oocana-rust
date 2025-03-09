## NPM package

current support platforms package:

- x86_64-apple-darwin
- aarch64-apple-darwin
- x86_64-unknown-linux-gnu
- aarch64-unknown-linux-gnu

> for other platform, you can add more platforms by adding more targets and toolchains

## cross compile prerequisites

### method 1: use `rustup target add` to add targets

```bash
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin
rustup target add x86_64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu

# cross compile for linux on macos
brew tap messense/macos-cross-toolchains
brew install x86_64-unknown-linux-gnu
brew install aarch64-unknown-linux-gnu
```

### method 2: use `cross` crate

```bash
cargo install cross
# cross only applies amd64 image, so you need use cross on amd64 machine other wise you need to use method 1 or build cross image yourself

# cross does not provide macos image, so you need to use method 1 or build cross image yourself by cross-toolchain

cross build --target x86_64-unknown-linux-gnu
cross build --target aarch64-unknown-linux-gnu
```

## build and publish

```bash
# update version in Cargo.toml
cd npm
node npm.mjs
node publish.mjs
```