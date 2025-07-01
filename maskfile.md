# Tasks For Build Flash-Cat

## release

> release flash-cat

```sh
mask build deb -a x86_64 
mask build deb -a aarch64 
mask build rpm -a aarch64
mask build rpm -a x86_64
mask build all
```

## build

> build flash-cat

### build all

> build all

```sh
mask build cli --arch x86_64 --target apple
mask build cli --arch x86_64 --target linux
mask build cli --arch x86_64 --target windows
mask build cli --arch aarch64 --target apple
mask build cli --arch aarch64 --target linux
mask build cli --arch i686 --target windows
```

### build cli

> build flash-cat cli

**OPTIONS**
* arch
    * flags: -a --arch
    * type: string
    * desc: Which arch to build
    * choices: x86_64, aarch64, i686
    * required
* target
    * flags: -t --target
    * type: string
    * desc: Which target to build
    * choices: apple, linux, windows
    * required

```sh
TARGET=""
if [[ "$arch" == "i686" && "$target" != "windows" ]]; then
    echo "Error: i686 architecture is only supported for Windows target"
    exit 1
fi
case "$target" in
    apple)
        TARGET="$arch-$target-darwin"
        ;;
    linux)
        TARGET="$arch-unknown-$target-musl"
        ;;
    windows)
        TARGET="$arch-pc-$target-gnu"
        ;;
esac

echo "Run build: cargo build --release -p flash_cat_cli --target $TARGET"
cargo build --release -p flash_cat_cli --target $TARGET
```


### build deb

> build deb package

**OPTIONS**
* arch
    * flags: -a --arch
    * type: string
    * desc: Which arch to build
    * choices: x86_64, aarch64
    * required

```sh
ARCH=""
case "$arch" in
  x86_64)
    ARCH="x86_64-unknown-linux-musl"
    ;;
  aarch64)
    ARCH="aarch64-unknown-linux-musl"
    ;;
  *)
    echo "Unsupported architecture: $arch"
    exit 1
    ;;
esac

cargo deb -p flash_cat_cli --variant $ARCH --target $ARCH
```

### build rpm

> build rpm package

**OPTIONS**
* arch
    * flags: -a --arch
    * type: string
    * desc: Which arch to build
    * choices: x86_64, aarch64
    * required

```sh
ARCH=""
case "$arch" in
  x86_64)
    ARCH="x86_64-unknown-linux-musl"
    ;;
  aarch64)
    ARCH="aarch64-unknown-linux-musl"
    ;;
  *)
    echo "Unsupported architecture: $arch"
    exit 1
    ;;
esac

cargo build --release -p flash_cat_cli --target $ARCH
cargo generate-rpm -p cli --variant $ARCH --target $ARCH
```

## clean

> clean build

```sh
cargo clean
```