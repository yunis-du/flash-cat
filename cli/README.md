# Flash-Cat-Cli

## Build RPM

**dependencies cargo-generate-rpm**

- install cargo-generate-rpm

```bash
cargo install cargo-generate-rpm 
```

### Build x86_64 RPM
- build x86_64-unknown-linux-musl

```bash
cargo build --release -p flash_cat_cli --target x86_64-unknown-linux-musl
```

- generate rpm package

```bash
cargo generate-rpm -p cli --variant x86_64-unknown-linux-musl --target x86_64-unknown-linux-musl 
```

### Build aarch64 RPM
- build aarch64-unknown-linux-musl

```bash
cargo build --release -p flash_cat_cli --target aarch64-unknown-linux-musl
```

- generate rpm package

```bash
cargo generate-rpm -p cli --variant aarch64-unknown-linux-musl --target aarch64-unknown-linux-musl
```

## Build Deb

**dependencies cargo-deb**

- install cargo-deb

```bash
cargo install cargo-deb
```

### Build x86_64 Deb

```bash
cargo deb -p flash_cat_cli --variant x86_64-unknown-linux-musl --target x86_64-unknown-linux-musl
```

### Build aarch64 Deb

```bash
cargo deb -p flash_cat_cli --variant aarch64-unknown-linux-musl --target aarch64-unknown-linux-musl
```
