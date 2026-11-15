#!/bin/sh
set -eu
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
chown 0:0 /

mkdir -p /proc /sys /dev /run /sys/fs/cgroup /mnt/state
mountpoint -q /proc || mount -t proc proc /proc
mountpoint -q /sys || mount -t sysfs sysfs /sys
mountpoint -q /dev || mount -t devtmpfs devtmpfs /dev
mountpoint -q /run || mount -t tmpfs tmpfs /run
chmod 755 /run
mountpoint -q /sys/fs/cgroup || mount -t cgroup2 cgroup2 /sys/fs/cgroup
ip link set lo up
ip link set eth0 up
udhcpc -i eth0 -n -q -t 20

if ! command -v mkfs.ext4 >/dev/null 2>&1; then
    apk add --no-cache e2fsprogs
fi
filesystem=$(blkid /dev/vda || true)
case "$filesystem" in
    '')
        if [ "$VMUX_NEW_DISK" != 1 ]; then
            echo "Existing data disk has no recognized filesystem" >&2
            exit 1
        fi
        mkfs.ext4 -F /dev/vda
        ;;
    *' TYPE="ext4"'*) ;;
    *) echo "Refusing unknown data disk filesystem: $filesystem" >&2; exit 1 ;;
esac
mount /dev/vda /mnt/state
mkdir -p /mnt/state/docker /mnt/state/apk
if ! command -v dockerd >/dev/null 2>&1 || ! command -v nft >/dev/null 2>&1 || ! command -v socat >/dev/null 2>&1 || ! command -v sshd >/dev/null 2>&1; then
    apk add --cache-dir /mnt/state/apk docker socat ca-certificates nftables openssh
fi

mkdir -p "$VMUX_SHARED_PATH"
mount -t virtiofs vmux-shared "$VMUX_SHARED_PATH"
sysctl -w net.ipv4.ip_forward=1
passwd -d root
ssh-keygen -A
mkdir -p /var/empty
mount -t tmpfs -o mode=0755 tmpfs /var/empty
cp /root/.ssh/authorized_keys /run/vmux-authorized-keys
chmod 600 /run/vmux-authorized-keys
/usr/sbin/sshd -t
/usr/sbin/sshd -D -e &
ssh_pid=$!

printf '%s\n' "$$" > /run/vmux-init.pid
socat VSOCK-LISTEN:2376,reuseaddr,fork EXEC:/sbin/vmux-control &
control_pid=$!
socat VSOCK-LISTEN:2375,reuseaddr,fork UNIX-CONNECT:/run/docker.sock &
socket_pid=$!
dockerd --host=unix:///run/docker.sock --data-root=/mnt/state/docker --firewall-backend=nftables &
docker_pid=$!

stop() {
    kill -TERM "$docker_pid" "$socket_pid" "$control_pid" "$ssh_pid" 2>/dev/null || true
    wait "$docker_pid" 2>/dev/null || true
    sync
}

trap stop EXIT
trap 'exit 0' TERM INT
wait "$docker_pid"
