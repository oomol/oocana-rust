set -xe

cd /workspaces/ovmlayer
$(pwd)/build.sh $(uname -m)
bin=$(find $(pwd) -name ovmlayer -type f)
ln -sf $bin /usr/local/bin/ovmlayer