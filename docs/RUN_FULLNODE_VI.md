# Huong dan chay DePINZcash Fullnode tren VPS

Tai lieu nay danh cho nguoi muon chay **Zebra full node** de tham gia DePINZcash. Script ben duoi se cai va chay fullnode Zcash bang Zebra, sau do chay `depinzcash-relay` de dang ky node va gui proof len DePINZcash API moi 5 phut.

## Yeu cau VPS

- He dieu hanh: Ubuntu 22.04/24.04 duoc khuyen nghi.
- CPU: toi thieu 2 core, khuyen nghi 4 core tro len.
- RAM: toi thieu 4 GB, khuyen nghi 8 GB.
- Disk: toi thieu 150 GB SSD, khuyen nghi NVMe va du phong them vi chain se tang dan.
- Network: ket noi on dinh, uptime cang cao diem thuong cang tot.
- Port nen mo: `8233/tcp` de Zebra nhan inbound peers.

## Cai dat nhanh

Chay cac lenh sau tren VPS moi:

```bash
git clone https://github.com/ZcashDePIN/DePINZcash.git
cd DePINZcash
chmod +x scripts/depinzcash-node.sh
./scripts/depinzcash-node.sh
```

Khi menu hien ra, chon:

```text
1) Cai va chay node
```

Script se tu dong:

- Cai cac goi can thiet.
- Cai Docker neu VPS chua co Docker.
- Cai Rust neu VPS chua co Rust.
- Build `depinzcash-relay`.
- Chay Zebra full node bang Docker image `zfnd/zebra:latest`.
- Bat Zebra RPC noi bo tai `127.0.0.1:8232`.
- Tao Solana keypair cho relay neu chua co.
- Dang ky node len `https://api.zcashdepin.com`.
- Tao systemd service `depinzcash-relay` de gui proof moi 5 phut.

## Menu cua script

```text
1) Cai va chay node
2) Xem logs
3) Xuat key vi
0) Thoat
```

### 1. Cai va chay node

Dung de cai moi hoac khoi dong lai setup. Neu keypair va relay state da ton tai, script se giu lai va khong tao key moi.

### 2. Xem logs

Co 3 lua chon:

- Logs relay DePINZcash.
- Logs Zebra full node.
- Trang thai service/container.

Lenh xem logs thu cong:

```bash
sudo journalctl -u depinzcash-relay -f
docker logs -f zebrad
```

Neu Docker yeu cau quyen root:

```bash
sudo docker logs -f zebrad
```

### 3. Xuat key vi

In ra public wallet, node ID va `keypair_b58`.

Canh bao: `keypair_b58` la private key. Khong gui cho bat ky ai, khong dang len Discord/Telegram/GitHub.

File key nam tai:

```bash
~/.depinzcash/config/solana-keypair.json
```

Relay state nam tai:

```bash
~/.depinzcash/config/relay-state.json
```

## Cac lenh kiem tra huu ich

Kiem tra Zebra container:

```bash
docker ps -a --filter name=zebrad
```

Kiem tra relay service:

```bash
sudo systemctl status depinzcash-relay --no-pager
```

Kiem tra Zebra RPC:

```bash
curl -s -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"1.0","id":"1","method":"getblockcount","params":[]}' \
  http://127.0.0.1:8232
```

## Chu y quan trong

- Khong chay script bang `sudo ./scripts/depinzcash-node.sh`. Hay chay binh thuong, script se tu goi `sudo` khi can.
- Neu VPS login bang `root`, co the chay script truc tiep.
- Lan build dau tien co the lau vi Rust phai compile RocksDB dependency.
- Zebra full node can thoi gian sync lau, thuong vai gio den hon mot ngay tuy VPS va network.
- Dung tat VPS trong luc sync neu co the.
- Nen backup file `~/.depinzcash/config/solana-keypair.json` o noi an toan.
- Neu xoa volume Docker `zebra-state`, Zebra se phai sync lai tu dau.
- Neu xoa keypair, node moi se dung vi moi va khong trung voi node da dang ky truoc do.

## Cau hinh nang cao

Mac dinh script dung API production:

```bash
https://api.zcashdepin.com
```

Co the doi API bang bien moi truong:

```bash
DEPINZCASH_API="https://api.example.com" ./scripts/depinzcash-node.sh
```

Co the doi thu muc luu key/config:

```bash
DEPINZCASH_HOME="$HOME/.depinzcash" ./scripts/depinzcash-node.sh
```

Co the doi ten Zebra container:

```bash
ZEBRA_CONTAINER="zebrad" ./scripts/depinzcash-node.sh
```
