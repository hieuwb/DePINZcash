# Huong dan chay DePINZcash Fullnode tren VPS

Tai lieu nay danh cho nguoi muon chay **Zebra full node** de tham gia DePINZcash. Script ben duoi se cai va chay fullnode Zcash bang Zebra, build `depinzcash-relay`, tao key vi Solana de ban dung tren web, va sau khi ban dang ky tren web xong thi script co the nhan `Node ID` + `Auth Token` de bat relay gui proof moi 5 phut.

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
git clone https://github.com/hieuwb/DePINZcash.git
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
- Khong tu dang ky node len API.
- Muc `2) Nhap Node ID va Auth Token` se tao relay state va bat service `depinzcash-relay`.
- Muc `3) Dang ky truc tiep bang terminal` se goi `depinzcash-relay register`, gap rate limit thi cho roi thu lai.
- Neu chua co `Node ID/Auth Token`, dung muc `5) Xuat key vi` de len web register truoc.
- Muc `6) Update script/node` se pull code moi, build lai relay, restart relay neu da cau hinh, va tuy chon update Zebra Docker image.

## Flow dung

### Buoc 1: Cai fullnode tren VPS

Chay script va chon:

```text
1) Cai va chay node
```

Sau khi cai xong, Zebra fullnode se bat dau sync. Quay lai menu de xuat key vi.

### Buoc 2: Xuat key vi

Chon:

```text
5) Xuat key vi
```

Script se in ra:

- `Wallet public key`
- `keypair_b58`

Canh bao: `keypair_b58` la private key. Khong gui cho ai.

### Buoc 3: Dang ky tren web

Len trang Register cua DePINZcash.

- Connect bang vi Solana tu key vua xuat.
- Ky message dang ky.
- Chu ky chi de chung minh quyen so huu vi, khong chuyen token.
- Sau khi dang ky, web se tra ve `Node ID` va `Auth Token`.

Chi dang ky tren web thoi chua du de nhan thuong. VPS van phai chay Zebra fullnode va relay phai gui proof moi 5 phut.

### Buoc 4A: Quay lai VPS de bat relay bang Node ID/Auth Token

Chay lai:

```bash
./scripts/depinzcash-node.sh
```

Chon:

```text
2) Nhap Node ID va Auth Token
```

Dan `Node ID` va `Auth Token` tu web vao. Script se tao:

```bash
~/.depinzcash/config/relay-state.json
```

va bat service:

```bash
depinzcash-relay
```

### Buoc 4B: Hoac dang ky truc tiep bang terminal

Neu khong muon dung web, chon:

```text
3) Dang ky truc tiep bang terminal
```

Muc nay dung keypair tren VPS de chay `depinzcash-relay register`. Neu API tra `429 Too Many Requests`, script se cho 60 giay roi thu lai. Bam `Ctrl+C` de dung.

Co the doi thoi gian cho bang bien moi truong:

```bash
REGISTER_RETRY_SECS=120 ./scripts/depinzcash-node.sh
```

## Menu cua script

```text
1) Cai va chay node
2) Nhap Node ID va Auth Token
3) Dang ky truc tiep bang terminal
4) Xem logs
5) Xuat key vi
6) Update script/node
7) Kiem tra trang thai node
8) Them node bang Docker Compose
0) Thoat
```

### 1. Cai va chay node

Dung de cai moi hoac khoi dong lai setup. Neu keypair da ton tai, script se giu lai va khong tao key moi.

### 2. Nhap Node ID va Auth Token

Dung sau khi da dang ky tren web. Muc nay se tao file relay state va bat service `depinzcash-relay` de gui proof moi 5 phut.

### 3. Dang ky truc tiep bang terminal

Dung neu muon bo qua web register. Muc nay tu goi `depinzcash-relay register`, neu thanh cong se luu relay state va bat service. Neu API dang qua tai va tra `429`, script cho 60 giay roi thu lai.

Khong nen retry 1 giay/lần vi no lam rate limit nang hon va co the khien IP bi chan lau hon.

### 4. Xem logs

Co 3 lua chon:

- Logs relay DePINZcash.
- Logs Zebra full node.
- Trang thai Zebra/RPC.

Lenh xem logs thu cong:

```bash
sudo journalctl -u depinzcash-relay -f
docker logs -f zebrad
```

Neu Docker yeu cau quyen root:

```bash
sudo docker logs -f zebrad
```

### 5. Xuat key vi

In ra public wallet, `keypair_b58`, va neu da cau hinh relay thi in them Node ID.

Dung public wallet de len web DePINZcash connect/register node.

Canh bao: `keypair_b58` la private key. Khong gui cho bat ky ai, khong dang len Discord/Telegram/GitHub.

File key nam tai:

```bash
~/.depinzcash/config/solana-keypair.json
```

### 6. Update script/node

Dung de cap nhat code moi tu GitHub:

- Chay `git fetch --all --prune`.
- Chay `git pull --ff-only`.
- Build lai `depinzcash-relay`.
- Restart/cap nhat service `depinzcash-relay` neu da co `relay-state.json`.
- Hoi rieng truoc khi pull image `zfnd/zebra:latest` va restart Zebra container.

Update khong xoa key vi, khong xoa `relay-state.json`, va khong xoa volume `zebra-state`.

Link xem commits moi:

```text
https://github.com/ZcashDePIN/DePINZcash/commits/main/
```

### 7. Kiem tra trang thai node

Dung de xem nhanh cac thong tin quan trong:

- Keypair va wallet local.
- `relay-state.json`, Node ID, API, label.
- Trang thai service `depinzcash-relay` va logs gan nhat.
- Trang thai Zebra Docker container va logs gan nhat.
- Zebra RPC local: `getblockcount`, `getbestblockhash`.
- API node status: `status`, `last_height`, `last_proof_at`, `points`, `uptime_seconds`.
- 5 proof gan nhat va ly do reject neu co.

Neu status tren API la `stale` nhung proof gan nhat van `accepted`, thuong la backend/API cap nhat cham hoac node dang sync cham. Kiem tra them logs relay va Zebra trong cung man hinh nay.

### 8. Them node bang Docker Compose

Dung khi muon chay them node thu 2, thu 3 tren cung VPS. Muc nay goi script rieng:

```bash
./scripts/depinzcash-compose-node.sh
```

Node phu se dung container, volume, port RPC, port P2P, relay state va systemd service rieng. Mac dinh node thu 2 dung:

- Container: `depinzcash-zebra-2`
- Volume: `depinzcash-zebra-2-state`
- RPC local: `http://127.0.0.1:18232`
- P2P port: `18233`
- Relay state: `~/.depinzcash/node-2/relay-state.json`
- Relay service: `depinzcash-relay-2`

Flow node thu 2:

```text
8) Them node bang Docker Compose
1) Chay Zebra node phu bang Docker Compose
2) Dang ky node phu truc tiep bang terminal
```

Khi dang ky node phu, dung cung wallet nhung label phai khac node cu, vi du `hieuwb-2`.

Luu y: node phu tren cung VPS phai dung port khac `8232/8233` de khong dung node chinh. P2P port khac `8233` van sync duoc bang outbound peers, nhung inbound peers co the it hon node chinh.

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
- Script khong register node tu dong trong muc cai node. Ban co the dung muc `5) Xuat key vi`, len web connect/register, lay `Node ID/Auth Token`, roi chay muc `2) Nhap Node ID va Auth Token`; hoac dung muc `3) Dang ky truc tiep bang terminal`.
- Dung tat VPS trong luc sync neu co the.
- Nen backup file `~/.depinzcash/config/solana-keypair.json` o noi an toan.
- Nen backup file `~/.depinzcash/config/relay-state.json` sau khi da dan `Node ID/Auth Token`.
- Neu xoa volume Docker `zebra-state`, Zebra se phai sync lai tu dau.
- Neu xoa keypair, node moi se dung vi moi va khong trung voi vi da dung tren web truoc do.
- Neu repo co thay doi local, muc update se canh bao truoc khi pull.

## Cau hinh nang cao

Co the doi thu muc luu key/config:

```bash
DEPINZCASH_HOME="$HOME/.depinzcash" ./scripts/depinzcash-node.sh
```

Co the doi ten Zebra container:

```bash
ZEBRA_CONTAINER="zebrad" ./scripts/depinzcash-node.sh
```
