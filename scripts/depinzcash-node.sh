#!/usr/bin/env bash
set -Eeuo pipefail

APP_NAME="DePINZcash"
REPO_URL="https://github.com/ZcashDePIN/DePINZcash.git"
REPO_DIR="${DEPINZCASH_REPO_DIR:-}"
INSTALL_DIR="${DEPINZCASH_HOME:-$HOME/.depinzcash}"
KEYPAIR="$INSTALL_DIR/config/solana-keypair.json"
ZEBRA_CONTAINER="${ZEBRA_CONTAINER:-zebrad}"
ZEBRA_VOLUME="${ZEBRA_VOLUME:-zebra-state}"
NODE_RPC="${NODE_RPC:-http://127.0.0.1:8232}"

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
  info "Script khong tu dang ky node. Hay dung muc 3 de xuat key vi, sau do len web de connect vi va register."
  info "Xem logs Zebra bang muc 2 trong menu hoac: docker logs -f $ZEBRA_CONTAINER"
}

show_logs() {
  echo
  echo "1) Logs Zebra fullnode"
  echo "2) Trang thai Zebra/RPC"
  read -r -p "Chon: " log_choice
  case "$log_choice" in
    1)
      if docker ps >/dev/null 2>&1; then
        docker logs -f "$ZEBRA_CONTAINER"
      else
        need_sudo docker logs -f "$ZEBRA_CONTAINER"
      fi
      ;;
    2)
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

  echo
  echo "Dung wallet public key tren de connect/register tren web."
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
