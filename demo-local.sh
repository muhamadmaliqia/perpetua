#!/usr/bin/env bash
# Perpetua — demo LOKAL satu perintah (nggak butuh faucet devnet).
# Jalanin:  bash demo-local.sh     (biarin idup), lalu di terminal lain: cd app && npm run dev
set -e
cd "$(dirname "$0")"

RPC=http://127.0.0.1:8899

echo "▶ Nyalain validator lokal…"
solana-test-validator --ledger /tmp/perpetua-ledger --reset >/tmp/perpetua-validator.log 2>&1 &
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
echo "      wallet lokal = burner otomatis (saldo 5 SOL diisi sendiri) — nggak perlu Phantom/faucet."
echo ""
echo "Validator jalan (PID $VPID). Ctrl-C buat berhenti."
wait $VPID
