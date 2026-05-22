#!/usr/bin/env bash
set -Eeuo pipefail

APP_NAME="DePINZcash"
REPO_URL="https://github.com/ZcashDePIN/DePINZcash.git"
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
      need_sudo apt-get install -y curl git ca-certificates build-essential clang libclang-dev pkg-config jq
      ;;
    dnf)
      need_sudo dnf install -y curl git ca-certificates gcc gcc-c++ clang clang-devel pkgconf-pkg-config jq
      ;;
    yum)
      need_sudo yum install -y curl git ca-certificates gcc gcc-c++ clang clang-devel pkgconfig jq
      ;;
    *)
      info "Khong nhan dien duoc package manager. Hay cai thu cong: curl git ca-certificates clang pkg-config jq."
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

register_if_needed() {
  if [[ -f "$STATE_FILE" ]]; then
    info "Relay state da ton tai: $STATE_FILE"
    return
  fi

  local label
  read -r -p "Nhap label node (Enter de dung 'primary'): " label
  label="${label:-primary}"

  mkdir -p "$(dirname "$STATE_FILE")"
  info "Dang register node len $API_ENDPOINT."

  local attempt=1
  local max_attempts=6
  local delay=30
  local log_file
  log_file="$(mktemp)"

  while (( attempt <= max_attempts )); do
    if "$REPO_DIR/prover/target/release/depinzcash-relay" register \
      --api "$API_ENDPOINT" \
      --keypair "$KEYPAIR" \
      --kind zebra-full \
      --label "$label" \
      --state "$STATE_FILE" 2>&1 | tee "$log_file"; then
      rm -f "$log_file"
      return
    fi

    if grep -qi "Too Many Requests\\|429" "$log_file"; then
      echo
      echo "API dang rate limit register. Thu lai lan $((attempt + 1))/$max_attempts sau ${delay}s..."
      sleep "$delay"
      delay=$((delay * 2))
      if (( delay > 300 )); then
        delay=300
      fi
      attempt=$((attempt + 1))
      continue
    fi

    rm -f "$log_file"
    die "Dang ky node that bai. Xem loi o tren."
  done

  rm -f "$log_file"
  die "API van tra 429 sau nhieu lan thu. Hay cho 10-30 phut roi chay lai muc 1; keypair hien tai van duoc giu tai $KEYPAIR."
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

install_and_run() {
  info "Bat dau cai dat va chay node."
  install_packages
  install_docker
  ensure_rust
  ensure_repo
  build_relay
  start_zebra
  keygen_if_needed
  register_if_needed
  install_systemd_service

  info "Hoan tat. Zebra dang sync; relay se submit moi 5 phut qua $NODE_RPC."
  info "Xem logs bang muc 2 trong menu hoac: journalctl -u $SERVICE_NAME -f"
}

show_logs() {
  echo
  echo "1) Logs relay DePINZcash"
  echo "2) Logs Zebra"
  echo "3) Trang thai services"
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
      if docker ps >/dev/null 2>&1; then
        docker ps -a --filter "name=$ZEBRA_CONTAINER"
      else
        need_sudo docker ps -a --filter "name=$ZEBRA_CONTAINER"
      fi
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

  if [[ -f "$STATE_FILE" ]] && command_exists jq; then
    echo "Wallet public key: $(jq -r '.wallet // empty' "$STATE_FILE")"
    echo "Node ID: $(jq -r '.node_id // empty' "$STATE_FILE")"
  fi

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
    echo "API : $API_ENDPOINT"
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
