#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
service=services/homekit
target=x86_64-unknown-linux-musl
build_dir="$PWD/.local/target"
output="$PWD/.local/packages"
manifest="$service/plugin.toml"

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$manifest")
cargo_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$service/Cargo.toml")
if [ -z "$version" ] || [ "$version" != "$cargo_version" ]; then
    echo "HomeKit package manifest version does not match Cargo.toml" >&2
    exit 1
fi

cargo build --locked --release --target "$target" --target-dir "$build_dir" \
    --manifest-path "$service/Cargo.toml" --bin homecore-homekit
mkdir -p "$output"
staging=$(mktemp -d "$output/.homekit.XXXXXXXX")
trap 'rm -rf -- "$staging"' EXIT
mkdir -p "$staging/files/ui"
install -m 755 "$build_dir/$target/release/homecore-homekit" "$staging/files/homecore-homekit"
install -m 644 "$manifest" "$service/config.default.toml" \
    "$service/LICENSE-MIT" "$service/LICENSE-APACHE" "$staging/files/"
install -m 644 "$service/ui/manifest.json" "$service/ui/index.js" "$staging/files/ui/"
cargo about generate --locked --fail --target "$target" --config deploy/licenses.toml \
    --manifest-path "$service/Cargo.toml" --output-file "$staging/files/THIRD-PARTY.txt" \
    deploy/licenses.hbs
install -m 644 "$(rustc --print sysroot)/share/doc/rust/COPYRIGHT-library.html" \
    "$staging/files/RUST-STANDARD-LIBRARY.html"
install -m 644 deploy/MUSL-COPYRIGHT "$staging/files/LICENSE-musl"

artifact="$output/plugin.homekit-$version-linux-x86_64.tar.zst"
tar --zstd --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
    -C "$staging/files" -cf "$staging/archive" .
mv -- "$staging/archive" "$artifact"
sha256sum "$artifact"
