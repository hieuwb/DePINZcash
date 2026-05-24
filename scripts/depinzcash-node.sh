#!/usr/bin/env bash
set -Eeuo pipefail

APP_NAME="DePINZcash"
REPO_URL="https://github.com/hieuwb/DePINZcash.git"
DEFAULT_API="https://api.zcashdepin.com"
REPO_DIR="${DEPINZCASH_REPO_DIR:-}"
INSTALL_DIR="${DEPINZCASH_HOME:-$HOME/.depinzcash}"
KEYPAIR="$INSTALL_DIR/config/solana-keypair.json"
STATE_FILE="$INSTALL_DIR/config/relay-state.json"
ZEBRA_CONTAINER="${ZEBRA_CONTAINER:-zebrad}"
ZEBRA_VOLUME="${ZEBRA_VOLUME:-zebra-state}"
SERVICE_NAME="depinzcash-relay"
NODE_RPC="${NODE_RPC:-http://127.0.0.1:8232}"
API_ENDPOINT="${DEPINZCASH_API:-$DEFAULT_API}"
REGISTER_RETRY_SECS="${REGISTER_RETRY_SECS:-60}"
UPDATE_COMMITS_URL="https://github.com/ZcashDePIN/DePINZcash/commits/main/"

if [[ -z "$REPO_DIR" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  if [[ -f "$SCRIPT_DIR/../prover/Cargo.toml" ]]; then
    REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
  else
    REPO_DIR="$INSTALL_DIR/DePINZcash"
  fi
fi

if [[ "${SUDO_USER:-}" && "${SUDO_USER:-}" != "root" ]]; then
  echo "Khong chay script bang sudo. Hay chay: ./scripts/depinzcash-node.sh"
  echo "Script se tu goi sudo khi can cai package, Docker hoac systemd."
  exit 1
fi

REAL_USER="$(id -un)"

need_sudo() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

info() {
  printf '\n[%s] %s\n' "$APP_NAME" "$*"
}

die() {
  echo "Loi: $*" >&2
  exit 1
}

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

detect_package_manager() {
  if command_exists apt-get; then
    echo "apt"
  elif command_exists dnf; then
    echo "dnf"
  elif command_exists yum; then
    echo "yum"
  else
    echo "unknown"
  fi
}

install_packages() {
  local pm
  pm="$(detect_package_manager)"
  case "$pm" in
    apt)
      need_sudo apt-get update
      need_sudo apt-get install -y curl git ca-certificates build-essential clang libclang-dev pkg-config jq python3
      ;;
    dnf)
      need_sudo dnf install -y curl git ca-certificates gcc gcc-c++ clang clang-devel pkgconf-pkg-config jq python3
      ;;
    yum)
      need_sudo yum install -y curl git ca-certificates gcc gcc-c++ clang clang-devel pkgconfig jq python3
      ;;
    *)
      info "Khong nhan dien duoc package manager. Hay cai thu cong: curl git ca-certificates clang pkg-config jq python3."
      ;;
  esac
}

install_docker() {
  if command_exists docker; then
    return
  fi

  info "Dang cai Docker bang script chinh thuc cua Docker."
  local docker_installer
  docker_installer="$(mktemp)"
  curl -fsSL https://get.docker.com -o "$docker_installer"
  need_sudo sh "$docker_installer"
  rm -f "$docker_installer"
  if [[ "$REAL_USER" != "root" ]]; then
    need_sudo usermod -aG docker "$REAL_USER" || true
  fi
}

ensure_docker_running() {
  need_sudo systemctl enable --now docker 2>/dev/null || true
  docker info >/dev/null 2>&1 || need_sudo docker info >/dev/null
}

ensure_rust() {
  if command_exists cargo; then
    return
  fi

  info "Dang cai Rust toolchain."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
}

ensure_repo() {
  if [[ -f "$REPO_DIR/prover/Cargo.toml" ]]; then
    return
  fi

  info "Dang clone repo DePINZcash vao $REPO_DIR."
  mkdir -p "$(dirname "$REPO_DIR")"
  git clone "$REPO_URL" "$REPO_DIR"
}

build_relay() {
  info "Dang build depinzcash-relay."
  (
    cd "$REPO_DIR/prover"
    cargo build --release --bin depinzcash-relay
  )
}

update_script_and_node() {
  info "Bat dau update DePINZcash."
  echo "Link xem update/commits: $UPDATE_COMMITS_URL"

  ensure_repo
  if ! command_exists git; then
    install_packages
  fi

  set +e
  (
    cd "$REPO_DIR"
    echo
    echo "Repo hien tai: $REPO_DIR"
    echo "Commit hien tai: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"

    if ! git diff --quiet || ! git diff --cached --quiet; then
      echo
      echo "Canh bao: repo dang co thay doi local."
      git status --short
      echo
      read -r -p "Van tiep tuc update bang git pull --ff-only? (y/N): " continue_update
      if [[ ! "$continue_update" =~ ^[Yy]$ ]]; then
        echo "Da huy update."
        exit 10
      fi
    fi

    git fetch --all --prune
    git pull --ff-only
    echo "Commit sau update: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
  )
  local update_status=$?
  set -e
  if [[ "$update_status" -eq 10 ]]; then
    return
  fi
  if [[ "$update_status" -ne 0 ]]; then
    die "Git update that bai. Neu repo bi diverged/local changes, hay xu ly thu cong roi chay lai."
  fi

  ensure_rust
  build_relay

  if [[ -f "$STATE_FILE" ]]; then
    install_systemd_service
    info "Da restart/cap nhat service $SERVICE_NAME."
  else
    info "Chua co relay-state.json, bo qua restart relay."
  fi

  echo
  read -r -p "Co pull image Zebra moi va restart container khong? (y/N): " update_zebra
  if [[ "$update_zebra" =~ ^[Yy]$ ]]; then
    ensure_docker_running
    if docker ps >/dev/null 2>&1; then
      docker pull zfnd/zebra:latest
    else
      need_sudo docker pull zfnd/zebra:latest
    fi
    start_zebra
    info "Da update/restart Zebra container. Volume $ZEBRA_VOLUME duoc giu lai."
  fi

  info "Update hoan tat."
}

start_zebra() {
  info "Dang khoi dong Zebra Docker container."
  ensure_docker_running

  if docker ps -a --format '{{.Names}}' | grep -qx "$ZEBRA_CONTAINER"; then
    docker rm -f "$ZEBRA_CONTAINER" >/dev/null
  elif ! docker ps >/dev/null 2>&1; then
    need_sudo docker rm -f "$ZEBRA_CONTAINER" >/dev/null 2>&1 || true
  fi

  local docker_cmd=(docker run -d
    --name "$ZEBRA_CONTAINER"
    --restart unless-stopped
    -e ZEBRA_RPC__LISTEN_ADDR=0.0.0.0:8232
    -e ZEBRA_RPC__ENABLE_COOKIE_AUTH=false
    -p 8233:8233
    -p 127.0.0.1:8232:8232
    -v "$ZEBRA_VOLUME:/home/zebra/.cache/zebra"
    zfnd/zebra:latest)

  if docker ps >/dev/null 2>&1; then
    "${docker_cmd[@]}"
  else
    need_sudo "${docker_cmd[@]}"
  fi
}

keygen_if_needed() {
  mkdir -p "$(dirname "$KEYPAIR")"
  if [[ -f "$KEYPAIR" ]]; then
    info "Keypair da ton tai: $KEYPAIR"
    return
  fi

  info "Dang tao Solana keypair moi cho relay."
  "$REPO_DIR/prover/target/release/depinzcash-relay" keygen --out "$KEYPAIR"
  chmod 600 "$KEYPAIR"
}

wallet_public_key() {
  python3 - "$KEYPAIR" <<'PY'
import json
import sys

alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def b58decode(value):
    n = 0
    for char in value:
        n *= 58
        if char not in alphabet:
            raise ValueError("invalid base58 character")
        n += alphabet.index(char)
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    pad = len(value) - len(value.lstrip("1"))
    return b"\x00" * pad + raw

def b58encode(raw):
    n = int.from_bytes(raw, "big")
    out = ""
    while n:
        n, rem = divmod(n, 58)
        out = alphabet[rem] + out
    pad = len(raw) - len(raw.lstrip(b"\x00"))
    return "1" * pad + (out or "")

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    keypair_b58 = json.load(fh)["keypair_b58"]
full = b58decode(keypair_b58)
if len(full) != 64:
    raise SystemExit(f"expected 64-byte keypair, got {len(full)}")
print(b58encode(full[32:]))
PY
}

write_relay_state() {
  local wallet="$1"
  local node_id="$2"
  local auth_token="$3"
  local label="$4"

  mkdir -p "$(dirname "$STATE_FILE")"
  python3 - "$STATE_FILE" "$API_ENDPOINT" "$wallet" "$node_id" "$auth_token" "$label" <<'PY'
import datetime
import json
import sys

path, api, wallet, node_id, auth_token, label = sys.argv[1:7]
payload = {
    "api": api,
    "wallet": wallet,
    "node_id": node_id,
    "auth_token": auth_token,
    "kind": "zebra-full",
    "label": label,
    "registered_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(payload, fh, indent=2)
    fh.write("\n")
PY
  chmod 600 "$STATE_FILE"
}

install_systemd_service() {
  local relay_bin="$REPO_DIR/prover/target/release/depinzcash-relay"
  local service_file="/etc/systemd/system/$SERVICE_NAME.service"
  local user_line=""

  if [[ "$REAL_USER" != "root" ]]; then
    user_line="User=$REAL_USER
Group=$REAL_USER"
  fi

  local tmp
  tmp="$(mktemp)"
  cat >"$tmp" <<EOF
[Unit]
Description=DePINZcash relay
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=simple
$user_line
WorkingDirectory=$REPO_DIR
Environment=RUST_LOG=info
Environment=DEPINZCASH_API=$API_ENDPOINT
ExecStart=$relay_bin watch --interval-secs 300 --api $API_ENDPOINT --keypair $KEYPAIR --state $STATE_FILE --node-rpc $NODE_RPC
Restart=always
RestartSec=20

[Install]
WantedBy=multi-user.target
EOF

  need_sudo install -m 0644 "$tmp" "$service_file"
  rm -f "$tmp"
  need_sudo systemctl daemon-reload
  need_sudo systemctl enable --now "$SERVICE_NAME"
}

configure_relay_from_web() {
  if [[ ! -f "$KEYPAIR" ]]; then
    echo "Chua co keypair. Hay chay muc 1 de cai va chay node truoc."
    return
  fi

  if [[ -f "$STATE_FILE" ]]; then
    info "Relay state da ton tai: $STATE_FILE"
    install_systemd_service
    info "Relay da duoc bat lai va se gui proof moi 5 phut."
    return
  fi

  echo
  echo "Sau khi dang ky tren web, ban se nhan Node ID va Auth Token."

  local node_id auth_token label wallet
  read -r -p "Nhap Node ID tu web: " node_id
  read -r -p "Nhap Auth Token tu web: " auth_token
  read -r -p "Nhap label node (Enter de dung 'primary'): " label
  label="${label:-primary}"

  [[ -n "$node_id" ]] || die "Node ID khong duoc de trong."
  [[ -n "$auth_token" ]] || die "Auth Token khong duoc de trong."

  wallet="$(wallet_public_key)"
  write_relay_state "$wallet" "$node_id" "$auth_token" "$label"
  info "Da luu relay state vao $STATE_FILE"
  install_systemd_service
  info "Relay da chay. No se gui proof len DePINZcash moi 5 phut."
}

register_from_terminal() {
  if [[ ! -x "$REPO_DIR/prover/target/release/depinzcash-relay" ]]; then
    echo "Chua co relay binary. Hay chay muc 1 de cai va build node truoc."
    return
  fi
  if [[ ! -f "$KEYPAIR" ]]; then
    echo "Chua co keypair. Hay chay muc 1 de tao keypair truoc."
    return
  fi
  if [[ -f "$STATE_FILE" ]]; then
    info "Relay state da ton tai: $STATE_FILE"
    install_systemd_service
    info "Relay da duoc bat lai va se gui proof moi 5 phut."
    return
  fi

  local label retry_secs attempt log_file
  read -r -p "Nhap label node (Enter de dung 'primary'): " label
  label="${label:-primary}"
  retry_secs="$REGISTER_RETRY_SECS"
  if ! [[ "$retry_secs" =~ ^[0-9]+$ ]] || (( retry_secs < 30 )); then
    retry_secs=60
  fi
  attempt=1
  log_file="$(mktemp)"

  echo
  echo "Dang ky truc tiep bang depinzcash-relay register."
  echo "Neu API tra 429, script se cho ${retry_secs}s roi thu lai. Bam Ctrl+C de dung."

  while true; do
    echo
    echo "Lan thu $attempt..."
    if "$REPO_DIR/prover/target/release/depinzcash-relay" register \
      --api "$API_ENDPOINT" \
      --keypair "$KEYPAIR" \
      --kind zebra-full \
      --label "$label" \
      --state "$STATE_FILE" 2>&1 | tee "$log_file"; then
      rm -f "$log_file"
      info "Dang ky thanh cong va da luu relay state vao $STATE_FILE"
      install_systemd_service
      info "Relay da chay. No se gui proof len DePINZcash moi 5 phut."
      return
    fi

    if grep -qi "Too Many Requests\\|429" "$log_file"; then
      echo "API dang rate limit. Cho ${retry_secs}s roi thu lai..."
      sleep "$retry_secs"
      attempt=$((attempt + 1))
      continue
    fi

    rm -f "$log_file"
    die "Dang ky that bai voi loi khong phai rate limit. Xem thong bao o tren."
  done
}

install_and_run() {
  info "Bat dau cai dat va chay node."
  install_packages
  install_docker
  ensure_rust
  ensure_repo
  build_relay
  start_zebra
  keygen_if_needed

  info "Hoan tat. Zebra fullnode dang sync."
  info "Neu da dang ky tren web, chon muc 2 de nhap Node ID/Auth Token va bat relay."
  info "Neu muon dang ky truc tiep bang terminal, chon muc 3."
  info "Neu chua dang ky, chon muc 5 de xuat key vi roi len web register."
}

show_logs() {
  echo
  echo "1) Logs relay DePINZcash"
  echo "2) Logs Zebra fullnode"
  echo "3) Trang thai Zebra/RPC"
  read -r -p "Chon: " log_choice
  case "$log_choice" in
    1) need_sudo journalctl -u "$SERVICE_NAME" -f --no-hostname ;;
    2)
      if docker ps >/dev/null 2>&1; then
        docker logs -f "$ZEBRA_CONTAINER"
      else
        need_sudo docker logs -f "$ZEBRA_CONTAINER"
      fi
      ;;
    3)
      need_sudo systemctl --no-pager status "$SERVICE_NAME" || true
      echo
      echo "Zebra container:"
      if docker ps >/dev/null 2>&1; then
        docker ps -a --filter "name=$ZEBRA_CONTAINER"
      else
        need_sudo docker ps -a --filter "name=$ZEBRA_CONTAINER"
      fi
      echo
      echo "Kiem tra RPC local:"
      curl -s -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"1.0","id":"1","method":"getblockcount","params":[]}' \
        "$NODE_RPC" || true
      echo
      ;;
    *) echo "Lua chon khong hop le." ;;
  esac
}

export_wallet_key() {
  echo
  echo "Vi tri keypair: $KEYPAIR"
  if [[ ! -f "$KEYPAIR" ]]; then
    echo "Chua co keypair. Hay chay muc 1 truoc."
    return
  fi

  if command_exists python3; then
    echo "Wallet public key: $(wallet_public_key)"
  else
    echo "Wallet public key: khong doc duoc vi VPS thieu python3."
  fi

  if [[ -f "$STATE_FILE" ]] && command_exists jq; then
    echo "Node ID: $(jq -r '.node_id // empty' "$STATE_FILE")"
    echo "Relay state: $STATE_FILE"
  fi

  echo
  echo "Huong dan dang ky web:"
  echo "1. Dung wallet public key/vi Solana nay de connect tren trang Register."
  echo "2. Ky message dang ky. Viec ky chi chung minh quyen so huu vi, khong chuyen token."
  echo "3. Web se tra ve Node ID va Auth Token."
  echo "4. Quay lai VPS, chon muc 2 va dan Node ID/Auth Token de bat relay."
  echo "Hoac chon muc 3 de dang ky truc tiep bang terminal."
  echo
  echo "CANH BAO: keypair_b58 ben duoi la private key. Khong gui cho bat ky ai."
  echo "-----BEGIN DEPINZCASH SOLANA KEYPAIR-----"
  if command_exists jq; then
    jq -r '.keypair_b58' "$KEYPAIR"
  else
    sed -n 's/.*"keypair_b58"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$KEYPAIR"
  fi
  echo "-----END DEPINZCASH SOLANA KEYPAIR-----"
}

main_menu() {
  while true; do
    clear
    echo "===================================="
    echo "  DePINZcash Node Installer"
    echo "===================================="
    echo "Repo: $REPO_DIR"
    echo "RPC : $NODE_RPC"
    echo
    echo "1) Cai va chay node"
    echo "2) Nhap Node ID va Auth Token"
    echo "3) Dang ky truc tiep bang terminal"
    echo "4) Xem logs"
    echo "5) Xuat key vi"
    echo "6) Update script/node"
    echo "0) Thoat"
    echo
    read -r -p "Chon: " choice
    case "$choice" in
      1) install_and_run; read -r -p "Nhan Enter de quay lai menu..." ;;
      2) configure_relay_from_web; read -r -p "Nhan Enter de quay lai menu..." ;;
      3) register_from_terminal; read -r -p "Nhan Enter de quay lai menu..." ;;
      4) show_logs ;;
      5) export_wallet_key; read -r -p "Nhan Enter de quay lai menu..." ;;
      6) update_script_and_node; read -r -p "Nhan Enter de quay lai menu..." ;;
      0) exit 0 ;;
      *) echo "Lua chon khong hop le."; sleep 1 ;;
    esac
  done
}

main_menu
