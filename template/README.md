# On-Demand Template Parachain

### Testing process

Setup: https://paritytech.github.io/zombienet/cli/setup.html
> NOTE: Get both polkadot and polkadot-parachain binaries.

Build the parachain:
```
cargo build --release
cp target/release/parachain-template-node .
```

Start the zombienet network:
```
zombienet-linux -p native spawn zombienet.toml
```
