#!/bin/bash
set -xe

# zsh history
ln -sf /root/commandhistory/.zsh_history /root/.zsh_history

# Heal a partially cached rustup toolchain in the shared /usr/local/cargo volume.
# We've seen rustup metadata report cargo as installed while the actual binary is
# missing, which makes any `cargo` command fail inside the container.
if ! cargo -V >/dev/null 2>&1; then
  active_toolchain=$(rustup show active-toolchain | awk '{print $1}')
  if [ -n "$active_toolchain" ]; then
    rustup toolchain uninstall "$active_toolchain" || true
    rustup toolchain install "$active_toolchain" --profile minimal -c cargo -c rustfmt -c clippy
  fi
fi

arch=$(dpkg --print-architecture)

# download ovmlayer
# this repo is private, make sure you have gh cli installed and authenticated
gh release download --repo oomol/ovmlayer-next --pattern "*${arch}*" --clobber -O /tmp/ovmlayer.tar.zst && \
zstd -d /tmp/ovmlayer.tar.zst -o /tmp/ovmlayer.tar && \
tar -xf /tmp/ovmlayer.tar -C /tmp
mv /tmp/ovmlayer/ovmlayer /usr/local/bin/ovmlayer
chmod +x /usr/local/bin/ovmlayer

# download ovmlayer rootfs
gh release download base-rootfs@0.4.0 --repo oomol/ovmlayer-rootfs --pattern "*${arch}*" --clobber -O /tmp/ubuntu-rootfs.tar

sudo ovmlayer setup --runtime /layer/runtime --external /layer/external --rootfs-tar /tmp/ubuntu-rootfs.tar --rootfs-path /layer/rootfs
