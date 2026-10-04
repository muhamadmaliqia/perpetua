# Perpetua — Demo & Video Runbook

Semua yang lu butuh buat **rekam video ≤3 menit + demo live** tanpa mikir lagi.
Urutan klik udah dikunci, narasi udah ditulis kata-per-kata, ada rencana cadangan.

---

## 0. Pre-flight (sekali doang, sebelum rekam)

- [ ] **Phantom** terpasang di browser, punya 1 wallet.
- [ ] Program ke-deploy (pilih salah satu jalur di §1).
- [ ] Frontend jalan: `cd ~/perpetua/app && npm run dev` → buka URL yang sesuai jalur.
- [ ] Wallet **connect** ke Perpetua, saldo cukup (≥2 SOL). Tombol `airdrop 2◎`
      nongol otomatis kalau saldo <1.
- [ ] Zoom browser 110–125% biar angka kebaca di video. Tutup tab/notif lain.
- [ ] **Reset pasar** kalau perlu demo dari nol: pakai wallet baru, atau feed lain
      (SOL/BTC/ETH) — tiap feed = pasar terpisah, jadi bisa "ignite" fresh.

---

## 1. Nyalain — dua jalur

### Jalur A — DEVNET (buat submission publik; butuh SOL devnet)
```bash
# 1) danai wallet dulu (INI yang keblok di CLI — pakai faucet web):
#    buka https://faucet.solana.com  → tempel pubkey GSgucw2zbkiFbFHs5vsLNe9vN2wEx9ahfwDsqC5zJnLe → minta 2 SOL
solana balance --url devnet          # pastiin ≥2 SOL masuk
# 2) deploy:
cd ~/perpetua && anchor deploy --provider.cluster devnet
# 3) frontend:
cd app && npm run dev                 # buka http://localhost:5173  (Phantom di jaringan Devnet)
```

### Jalur B — LOKAL (nggak butuh faucet SAMA SEKALI — buat rekam/latihan)
```bash
# terminal 1 (biarin idup):
cd ~/perpetua && bash demo-local.sh
# terminal 2:
cd ~/perpetua/app && npm run dev
# buka: http://localhost:5173?rpc=http://localhost:8899
# di Phantom: Settings > Developer > tambah custom RPC http://localhost:8899, pilih jaringan itu
# klik tombol "airdrop 2◎" di app (bebas di lokal)
```
> Link Explorer di app otomatis nyesuain: di lokal → `cluster=custom`, di devnet → `cluster=devnet`.

---

## 2. Urutan klik demo (hafalin — ini "jalur emas")

**Babak 1 — IGNITE (bebas-izin, ~10 dtk)**
1. Pilih feed **SOL/USD** (chip).
2. Klik **⚡ NYALAKAN PASAR**. Pasar hidup — mark price ke-seed on-chain, oracle = feed Pyth.

**Babak 2 — TRADE + PnL LIVE (yang menang)**
3. LONG · jaminan **0.5** · leverage **5** → klik **BUKA**.
4. Posisi kebuka — **PnL kehitung live** dari mark on-chain (poll ~1.5 dtk). Tunjuk angkanya.
5. Klik **pump +25% (demo)** → mark naik, **PnL ijo ~+125%** (5x) — momen menang. (Jalur set_mark_price,
   1 tanda tangan; demo nggak narik harga live eksternal — pump/crash yg gerakin harga.)
6. Klik **TUTUP POSISI** → payout **gede** masuk wallet. Link tx muncul (devnet: Explorer ↗ · localnet: signature).

**Babak 3 — LIKUIDASI (bebas-izin, jaga solvent)**
7. Buka posisi lagi: LONG · 0.5 · **5x** · **BUKA**.
8. Klik **crash -25% (demo)** → keeper pause, mark anjlok, **PnL merah ~-125%**.
9. Klik **likuidasi (demo)** → posisi ketutup paksa, **bounty masuk**. (Tekankan:
   *siapa pun* bisa lakuin ini, bukan cuma lu — itu yang bikin pool aman tanpa operator.)

> Kenapa -25% & 5x: batas maintenance = 5% notional. 5x long turun 25% →
> ekuitas < 0 → pasti keliadasi. Kalau lu ganti leverage, sesuaikan crash-nya.

---

## 3. Skrip video (≤3 mnt, kata-per-kata)

> Narasi **English** (jangkauan juri global). `[LAYAR]` = yang keliatan, `[LAKU]` = yang lu klik.
> Total ~2:40. Ngomong pelan; angka biarin kebaca.

**0:00–0:18 — HOOK + MASALAH**
[LAYAR: halaman Perpetua, "THE MARKET NEVER CLOSES"]
> "Every perps exchange ships the same ten markets — SOL, BTC, ETH. But the hard
> part was never *trading* a market. It's *creating* one. What if anyone could
> launch a leveraged market on **any number** — in ten seconds, permissionless?"

**0:18–0:45 — IGNITE (wow)**
[LAKU: pilih SOL → klik ⚡ NYALAKAN PASAR]
> "This is Perpetua. I pick a Pyth feed, and I ignite. Ten seconds later a live,
> leveraged market exists on-chain — no orderbook, no listing committee, no
> market makers. Anyone can do this, for any feed Pyth carries."
[LAYAR: kartu pasar muncul, mark price on-chain, oracle = Pyth feed]

**0:45–1:20 — TRADE + LIVE PnL (kenapa Solana)**
[LAKU: LONG, 0.5 SOL, 5x → BUKA → pump +25%]
> "The counterparty isn't another trader — it's an automated liquidity pool, a
> vAMM. I open a five-x long with half a SOL. The market moves, and my PnL updates
> live from the on-chain mark. Every ignite, every price move, every settlement is
> a real Solana transaction confirmed in well under a second — that's why this
> lives on Solana."
[LAYAR: PnL loncat ijo +125%]

**1:20–1:45 — CLOSE + PAYOUT (settle on-chain)**
[LAKU: TUTUP POSISI → klik link Explorer]
> "Close, and the payout settles on-chain, straight to my wallet. Every action is
> a real Solana transaction — here it is on the explorer."

**1:45–2:20 — LIKUIDASI (bebas-izin, solvent)**
[LAKU: BUKA lagi 5x → crash -25% → likuidasi]
> "Now the important part: risk. Say the price crashes. My margin's wiped — I'm
> underwater. On Perpetua, liquidation is **permissionless** — *anyone* can close
> an unhealthy position and earn a bounty. The pool stays solvent with no central
> operator. That's the whole game."

**2:20–2:40 — VISI + TUTUP**
[LAYAR: balik ke daftar feed / logo]
> "Perps and prediction markets have always been separate products. Perpetua makes
> them **one primitive**: if a number has a feed, you can trade leverage on it —
> right now, permissionlessly. Today SOL, BTC, ETH. Tomorrow, any number on Earth.
> **Perpetua. The market never closes.**"

---

## 4. Tips rekam
- Layar 1080p+, browser zoom 110–125%, kursor gede kalau bisa.
- Rekam per babak, gabung nanti — kalau satu babak nge-lag, ulang babak itu aja.
- OBS / QuickTime / Loom oke. Mic jelas > kualitas gambar.
- Sisipin close-up 1–2 detik ke **PnL ticking** dan ke **tx Explorer** — itu bukti "beneran on-chain".
- Kalau pengen versi Indonesia: narasi di atas gampang dialih — arti sama, tetep ≤3 mnt.

## 5. Kalau ngadat (recovery)
- **Tombol nggak jalan / "Gagal"** → lihat baris log paling bawah app; cek Phantom
  di jaringan yang bener (devnet vs localhost) & saldo cukup.
- **PnL nggak gerak** → normal: harga mark cuma berubah pas klik **pump/crash (demo)** atau buka/tutup
  posisi (demo nggak narik harga live eksternal). Klik **pump +25%** buat gerakin.
- **Likuidasi bilang "masih sehat"** → crash-nya kurang dalem buat leverage itu;
  klik **crash -25%** lagi, atau naikin leverage posisi ke 5x+.
- **Devnet mahal/lambat pas rekam** → pindah ke **Jalur B (lokal)**, tampilannya identik.
- **Validator lokal error** → `pkill -f solana-test-validator` lalu `bash demo-local.sh` lagi.

## 6. Checklist submit
- [ ] Video ≤3 mnt ke-upload (publik: YouTube/Loom unlisted-public).
- [ ] Repo GitHub publik (commit pakai identitas **bre+temen**, **TANPA** "Co-Authored-By: Claude").
- [ ] README ke-baca (wedge, arsitektur, cara jalanin, why-Solana) — udah siap.
- [ ] Link live/demo kalau ada (devnet). Kalau nggak, video + repo cukup.
- [ ] Satu submission per tim. Cek Rules/Discord buat field wajib + aturan kode-AI.
