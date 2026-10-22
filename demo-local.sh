#!/usr/bin/env bash
# Perpetua: demo LOKAL satu perintah (nggak butuh faucet devnet).
# Jalanin:  bash demo-local.sh     (biarin idup), lalu di terminal lain: cd app && npm run dev
set -e
cd "$(dirname "$0")"

RPC=http://127.0.0.1:8899

# Harga ASLI di demo lokal: clone akun harga Pyth sponsored dari devnet (XAU/SOL/BTC/ETH, owner Pyth Receiver)
# -> app mulai tiap pasar dari harga Pyth terakhir (beku pas clone). Offline -> jalan tanpa clone (harga cadangan).
CLONES=""
if curl -s -m 5 -o /dev/null https://api.devnet.solana.com; then
  CLONES="--url https://api.devnet.solana.com"
  for ACCT in 2uPQGpm8X4ZkxMHxrAW1QuhXcse1AHEgPih6Xp9NuEWW 7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE \
              4cSM2e6rvbGQUFiJbqytoVMi5GgghSMr8LwVrT9VPSPo 42amVS4KgzR9rA28tkVYqVXjq9Qa8dcZQMbH5EYFX6XC; do
    CLONES="$CLONES --clone $ACCT"
  done
  echo "▶ Clone harga Pyth asli dari devnet (XAU/SOL/BTC/ETH)…"
else
  echo "▶ Devnet nggak kejangkau, jalan tanpa clone (pakai harga cadangan)."
fi

echo "▶ Nyalain validator lokal…"
# shellcheck disable=SC2086 # CLONES sengaja dipecah jadi beberapa argumen
solana-test-validator --ledger /tmp/perpetua-ledger --reset $CLONES >/tmp/perpetua-validator.log 2>&1 &
VPID=$!

echo "  nunggu RPC siap…"
until solana cluster-version --url "$RPC" >/dev/null 2>&1; do sleep 1; done

echo "▶ Danai wallet (lokal, bebas) + deploy program…"
solana airdrop 100 --url "$RPC" >/dev/null
anchor deploy --provider.cluster localnet

echo ""
echo "✅ Program ke-deploy di localnet."
echo "   1) terminal lain:  cd app && npm run dev"
echo "   2) buka browser:   http://localhost:5173/?rpc=$RPC"
echo "      wallet lokal = burner otomatis (saldo 5 SOL diisi sendiri), nggak perlu Phantom/faucet."
echo ""
echo "Validator jalan (PID $VPID). Ctrl-C buat berhenti."
wait $VPID
