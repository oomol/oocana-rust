!#/bin/bash
set -xe

# zsh history
ln -sf /root/commandhistory/.zsh_history /root/.zsh_history

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

# setup ovmlayer
sudo ovmlayer setup -d /workspaces/ovmlayer-layers,rw --rootfs-tar /tmp/ubuntu-rootfs.tar --rootfs-path /workspaces/rootfs

# cleanup temporary archives and extracted ovmlayer directory to free space
rm -f /tmp/ovmlayer.tar.zst /tmp/ovmlayer.tar /tmp/ubuntu-rootfs.tar && rm -rf /tmp/ovmlayer