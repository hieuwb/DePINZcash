#!/usr/bin/env bash
set -Eeuo pipefail

APP_NAME="DePINZcash Compose Node"
DEFAULT_API="https://api.zcashdepin.com"
REPO_DIR="${DEPINZCASH_REPO_DIR:-}"
INSTALL_DIR="${DEPINZCASH_HOME:-$HOME/.depinzcash}"
KEYPAIR="$INSTALL_DIR/config/solana-keypair.json"
API_ENDPOINT="${DEPINZCASH_API:-$DEFAULT_API}"
REGISTER_RETRY_SECS="${REGISTER_RETRY_SECS:-60}"
REGISTER_FORBIDDEN_RETRY_SECS="${REGISTER_FORBIDDEN_RETRY_SECS:-300}"

if [[ -z "$REPO_DIR" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
fi

if [[ "${SUDO_USER:-}" && "${SUDO_USER:-}" != "root" ]]; then
  echo "Khong chay script bang sudo. Hay chay: ./scripts/depinzcash-compose-node.sh"
  echo "Script se tu goi sudo khi can systemd hoac Docker."
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

compose_cmd() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  elif command_exists docker-compose; then
    docker-compose "$@"
  else
    die "Khong tim thay Docker Compose. Hay cai Docker Compose plugin truoc."
  fi
}

compose_cmd_sudo() {
  if docker ps >/dev/null 2>&1; then
    compose_cmd "$@"
  elif need_sudo docker compose version >/dev/null 2>&1; then
    need_sudo docker compose "$@"
  elif command_exists docker-compose; then
    need_sudo docker-compose "$@"
  else
    die "Khong tim thay Docker Compose. Hay cai Docker Compose plugin truoc."
  fi
}

ensure_relay_binary() {
  if [[ -x "$REPO_DIR/prover/target/release/depinzcash-relay" ]]; then
    return
  fi
  info "Dang build depinzcash-relay."
  (
    cd "$REPO_DIR/prover"
    cargo build --release --bin depinzcash-relay
  )
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

set_instance() {
  local input_index="${1:-}"
  if [[ -z "$input_index" ]]; then
    read -r -p "Nhap so thu tu node phu (Enter de dung 2): " input_index
  fi
  NODE_INDEX="${input_index:-2}"
  [[ "$NODE_INDEX" =~ ^[0-9]+$ ]] || die "Node index phai la so."
  (( NODE_INDEX >= 2 )) || die "Node index phu nen bat dau tu 2."

  INSTANCE_DIR="$INSTALL_DIR/node-$NODE_INDEX"
  STATE_FILE="$INSTANCE_DIR/relay-state.json"
  ENV_FILE="$INSTANCE_DIR/compose.env"
  SERVICE_NAME="depinzcash-relay-$NODE_INDEX"
  ZEBRA_CONTAINER="depinzcash-zebra-$NODE_INDEX"
  ZEBRA_VOLUME="depinzcash-zebra-$NODE_INDEX-state"
  ZEBRA_RPC_PORT="${ZEBRA_RPC_PORT:-$((18000 + NODE_INDEX * 100 + 32))}"
  ZEBRA_P2P_PORT="${ZEBRA_P2P_PORT:-$((18000 + NODE_INDEX * 100 + 33))}"
  NODE_RPC="http://127.0.0.1:$ZEBRA_RPC_PORT"
  PROJECT_NAME="depinzcash-node$NODE_INDEX"
}

write_compose_env() {
  mkdir -p "$INSTANCE_DIR"
  cat >"$ENV_FILE" <<EOF
ZEBRA_CONTAINER=$ZEBRA_CONTAINER
ZEBRA_VOLUME=$ZEBRA_VOLUME
ZEBRA_RPC_PORT=$ZEBRA_RPC_PORT
ZEBRA_P2P_PORT=$ZEBRA_P2P_PORT
EOF
}

start_compose_node() {
  set_instance "${1:-}"
  write_compose_env
  info "Dang khoi dong Zebra node $NODE_INDEX bang Docker Compose."
  echo "RPC local: $NODE_RPC"
  echo "P2P port : $ZEBRA_P2P_PORT"
  echo "Volume   : $ZEBRA_VOLUME"
  compose_cmd_sudo --env-file "$ENV_FILE" -f "$REPO_DIR/docker-compose.extra-node.yml" -p "$PROJECT_NAME" up -d
  info "Da chay Zebra node $NODE_INDEX."
}

write_relay_state() {
  local wallet="$1"
  local node_id="$2"
  local auth_token="$3"
  local label="$4"

  mkdir -p "$INSTANCE_DIR"
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

install_relay_service() {
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
Description=DePINZcash relay $NODE_INDEX
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

configure_from_web() {
  set_instance "${1:-}"
  ensure_relay_binary
  [[ -f "$KEYPAIR" ]] || die "Chua co keypair $KEYPAIR. Hay chay script node chinh truoc."

  if [[ -f "$STATE_FILE" ]]; then
    info "Relay state da ton tai: $STATE_FILE"
    install_relay_service
    return
  fi

  local node_id auth_token label wallet
  read -r -p "Nhap Node ID node $NODE_INDEX tu web: " node_id
  read -r -p "Nhap Auth Token node $NODE_INDEX tu web: " auth_token
  read -r -p "Nhap label node (Enter de dung node-$NODE_INDEX): " label
  label="${label:-node-$NODE_INDEX}"

  [[ -n "$node_id" ]] || die "Node ID khong duoc de trong."
  [[ -n "$auth_token" ]] || die "Auth Token khong duoc de trong."

  wallet="$(wallet_public_key)"
  write_relay_state "$wallet" "$node_id" "$auth_token" "$label"
  install_relay_service
  info "Da bat relay $SERVICE_NAME cho node $NODE_INDEX."
}

register_from_terminal() {
  set_instance "${1:-}"
  ensure_relay_binary
  [[ -f "$KEYPAIR" ]] || die "Chua co keypair $KEYPAIR. Hay chay script node chinh truoc."

  if [[ -f "$STATE_FILE" ]]; then
    info "Relay state da ton tai: $STATE_FILE"
    install_relay_service
    return
  fi

  local label retry_secs forbidden_retry_secs attempt log_file
  read -r -p "Nhap label node (Enter de dung node-$NODE_INDEX): " label
  label="${label:-node-$NODE_INDEX}"
  retry_secs="$REGISTER_RETRY_SECS"
  if ! [[ "$retry_secs" =~ ^[0-9]+$ ]] || (( retry_secs < 30 )); then
    retry_secs=60
  fi
  forbidden_retry_secs="$REGISTER_FORBIDDEN_RETRY_SECS"
  if ! [[ "$forbidden_retry_secs" =~ ^[0-9]+$ ]] || (( forbidden_retry_secs < 60 )); then
    forbidden_retry_secs=300
  fi
  attempt=1
  log_file="$(mktemp)"

  echo "Dang ky node $NODE_INDEX bang label '$label'. Bam Ctrl+C de dung."
  echo "Neu API tra 403 do tam dong dang ky, script se cho ${forbidden_retry_secs}s roi thu lai."
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
      install_relay_service
      info "Dang ky thanh cong va da bat relay $SERVICE_NAME."
      return
    fi

    if grep -qi "Too Many Requests\\|429" "$log_file"; then
      echo "API dang rate limit. Cho ${retry_secs}s roi thu lai..."
      sleep "$retry_secs"
      attempt=$((attempt + 1))
      continue
    fi

    if grep -qi "403 Forbidden\\|\"error\":\"forbidden\"" "$log_file"; then
      echo "API dang tu choi dang ky (403 Forbidden). Co the server dang tat REGISTRATION_ENABLED/tam dong dang ky."
      echo "Cho ${forbidden_retry_secs}s roi thu lai. Bam Ctrl+C de dung."
      sleep "$forbidden_retry_secs"
      attempt=$((attempt + 1))
      continue
    fi

    rm -f "$log_file"
    die "Dang ky that bai voi loi khong phai rate limit."
  done
}

show_status() {
  set_instance "${1:-}"
  write_compose_env
  echo
  echo "== Compose node $NODE_INDEX =="
  echo "RPC local: $NODE_RPC"
  echo "P2P port : $ZEBRA_P2P_PORT"
  echo "State    : $STATE_FILE"
  echo
  compose_cmd_sudo --env-file "$ENV_FILE" -f "$REPO_DIR/docker-compose.extra-node.yml" -p "$PROJECT_NAME" ps || true
  echo
  echo "RPC height:"
  curl -s --max-time 10 -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"1.0","id":"compose-node","method":"getblockcount","params":[]}' \
    "$NODE_RPC" || true
  echo
  echo
  need_sudo systemctl --no-pager status "$SERVICE_NAME" || true
  echo
  need_sudo journalctl -u "$SERVICE_NAME" -n 30 --no-pager --no-hostname || true
}

main_menu() {
  while true; do
    clear
    echo "========================================="
    echo "  DePINZcash Extra Node Docker Compose"
    echo "========================================="
    echo "Keypair: $KEYPAIR"
    if [[ -f "$KEYPAIR" ]] && command_exists python3; then
      echo "Wallet : $(wallet_public_key)"
    fi
    echo
    echo "1) Chay Zebra node phu bang Docker Compose"
    echo "2) Dang ky node phu truc tiep bang terminal"
    echo "3) Nhap Node ID/Auth Token cho node phu"
    echo "4) Xem trang thai/logs node phu"
    echo "0) Thoat"
    echo
    read -r -p "Chon: " choice
    case "$choice" in
      1) start_compose_node; read -r -p "Nhan Enter de quay lai menu..." ;;
      2) register_from_terminal; read -r -p "Nhan Enter de quay lai menu..." ;;
      3) configure_from_web; read -r -p "Nhan Enter de quay lai menu..." ;;
      4) show_status; read -r -p "Nhan Enter de quay lai menu..." ;;
      0) exit 0 ;;
      *) echo "Lua chon khong hop le."; sleep 1 ;;
    esac
  done
}

main_menu
