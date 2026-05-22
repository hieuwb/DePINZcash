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
  if [[ -f "$STATE_FILE" ]]; then
    info "Relay state da ton tai: $STATE_FILE"
    install_systemd_service
    info "Relay da duoc bat lai va se gui proof moi 5 phut."
    return
  fi

  echo
  echo "Sau khi dang ky tren web, ban se nhan Node ID va Auth Token."
  echo "Neu chua dang ky web, chon 'n', sau do dung muc 3 de xuat key vi."
  read -r -p "Ban da co Node ID/Auth Token tu web chua? (y/N): " has_credentials
  if [[ ! "$has_credentials" =~ ^[Yy]$ ]]; then
    echo
    echo "Buoc tiep theo:"
    echo "1. Chon muc 3 de xuat key vi."
    echo "2. Len web DePINZcash de connect/register bang vi do."
    echo "3. Lay Node ID va Auth Token tren web."
    echo "4. Chay lai muc 1, nhap Node ID/Auth Token de bat relay."
    return
  fi

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

install_and_run() {
  info "Bat dau cai dat va chay node."
  install_packages
  install_docker
  ensure_rust
  ensure_repo
  build_relay
  start_zebra
  keygen_if_needed
  configure_relay_from_web

  info "Hoan tat. Zebra fullnode dang sync."
  info "Xem logs bang muc 2 trong menu."
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
  echo "4. Quay lai VPS, chay muc 1 va dan Node ID/Auth Token de bat relay."
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
    echo "2) Xem logs"
    echo "3) Xuat key vi"
    echo "0) Thoat"
    echo
    read -r -p "Chon: " choice
    case "$choice" in
      1) install_and_run; read -r -p "Nhan Enter de quay lai menu..." ;;
      2) show_logs ;;
      3) export_wallet_key; read -r -p "Nhan Enter de quay lai menu..." ;;
      0) exit 0 ;;
      *) echo "Lua chon khong hop le."; sleep 1 ;;
    esac
  done
}

main_menu
