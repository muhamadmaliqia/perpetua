# Perpetua — Demo & Video Runbook

Semua yang lu butuh buat **rekam video ≤3 menit + demo live** tanpa mikir lagi.
Urutan klik udah dikunci, narasi udah ditulis kata-per-kata, ada rencana cadangan.

> Diperbarui 21 Okt 2026: label tombol = UI sekarang (English) · wallet lokal = burner otomatis · babak baru
> **MESIN** (ukuran order gerakin harga + funding) dan **PREDIKSI** (Pyth yang mutusin). Angka panggung dikunci
> tes — tabel di §2.

---

## 0. Pre-flight (sekali doang, sebelum rekam)

- [ ] Program ke-deploy (pilih jalur di §1).
- [ ] Frontend jalan: `cd ~/perpetua/app && npm run dev` → buka URL sesuai jalur.
- [ ] **Lokal:** wallet = **burner otomatis** (pojok kanan atas `… · burner`), saldo 5 SOL diisi sendiri — nol
      Phantom, nol faucet. `demo-local.sh` nyalin akun harga Pyth ASLI dari devnet (butuh internet) → tiap pasar
      mulai dari harga Pyth terakhir (beku pas validator dinyalain). Offline → harga cadangan. **Devnet:** Phantom di jaringan Devnet, saldo ≥2 SOL (tombol `airdrop 2 SOL` nongol
      kalau saldo <1).
- [ ] Zoom browser 110–125% biar angka kebaca di video. Tutup tab/notif lain.
- [ ] **Pasar fresh:** satu feed = satu pasar. Mau ulang dari nol → restart `bash demo-local.sh` (ledger di-reset),
      atau pakai feed lain.
- [ ] Sebelum take: `bash e2e-local.sh` → mesti **4/4 OK** (jalur + angka panggung masih kebukti jalan).

---

## 1. Nyalain — dua jalur

### Jalur A — DEVNET (link publik buat juri + babak PREDIKSI pakai harga Pyth asli; butuh SOL devnet)
```bash
# 1) danai wallet dulu (CLI airdrop sering dijatah — pakai faucet web):
#    https://faucet.solana.com → tempel GSgucw2zbkiFbFHs5vsLNe9vN2wEx9ahfwDsqC5zJnLe → minta 2 SOL
solana balance --url devnet          # pastiin ≥2 SOL masuk
# 2) deploy:
cd ~/perpetua && anchor deploy --provider.cluster devnet
# 3) frontend:
cd app && npm run dev                 # buka http://localhost:5173  (Phantom di jaringan Devnet)
```

### Jalur B — LOKAL (nol faucet — buat rekam/latihan babak 1–3)
```bash
cd ~/perpetua && bash demo-local.sh        # terminal 1, biarin idup
cd ~/perpetua/app && npm run dev          # terminal 2
# buka: http://localhost:5173/?rpc=http://127.0.0.1:8899   (burner otomatis, saldo 5 SOL)
```
> Link tx otomatis nyesuain: lokal → signature di status bar, devnet → Solana Explorer.
> Lokal **nggak punya akun Pyth** → babak PREDIKSI cuma bisa resolve di devnet (Babak 4).

---

## 2. Urutan klik demo (hafalin — ini "jalur emas")

**Babak 1 — IGNITE (bebas-izin, ~10 dtk)**
1. Kiri: pilih **XAU/USD** (paling atas). Kotak **ANY PYTH FEED**: **PERP**.
2. Klik **IGNITE MARKET** → pasar hidup di **harga Pyth terakhir** (baris oracle: "latest Pyth price published …"),
   seed likuiditas 2 SOL, oracle = feed Pyth.

**Babak 2 — TRADE: ukuran order gerakin harga + PnL live**
3. Order ticket (kanan): **LONG · 0.5 · 5×**. Tunjuk **Est. fill** + **Price impact +5.00%** — harga isi naik karena
   ukuran order lu sendiri, bukan diketik siapa-siapa.
4. Klik **OPEN LONG**. Strip **SKEW** di tengah jadi **LONG-HEAVY**, fill premium **+10.00%**; **FUNDING / HOUR**
   naik detik demi detik ("longs pay shorts").
5. Klik **pump +25%** (strip `AUTHORITY · DEMO`) → **PnL ijo ≈ +125%** (5×).
6. Klik **CLOSE POSITION** → payout **≈ 1.125 SOL** masuk wallet (jaminan 0.5 + PnL 0.625, dipotong funding
   beberapa lamport). Tx muncul di status bar.

**Babak 3 — LIKUIDASI (bebas-izin)**
7. **OPEN LONG** lagi (0.5 · 5×). **Liq price** kebaca di panel kanan (≈ −15% dari entry).
8. Klik **crash −25%** → health **AT RISK**, PnL ≈ −125%.
9. Klik **LIQUIDATE (demo)** → posisi ketutup paksa, bounty **0.025 SOL** (1% notional). Tekankan: *siapa pun*
   bisa lakuin ini.

> Kenapa −25% & 5×: maintenance = 5% notional. Long 5× turun 15% = liq price; turun 25% → ekuitas < 0 → pasti
> kelikuidasi. Ganti leverage → sesuaikan crash-nya.

**Babak 4 — PREDIKSI: Pyth yang mutusin (DEVNET — butuh akun Pyth asli)**
10. Pilih **SOL/USD** (atau BTC/ETH/XAU — **US500 nggak punya akun Pyth di devnet**). Kotak ANY PYTH FEED:
    **PREDICTION · 1M** → **IGNITE PREDICTION**. Harga awal = harga Pyth terakhir buat feed itu. Tiap ronde = pasar
    sendiri: begitu ronde selesai, tombol jadi **NEW ROUND** (feed nggak pernah "abis"). Link langsung ke tampilan ini:
    `…/?feed=SOL/USD&view=prediction`.
11. **OPEN LONG** (taruhan naik) atau SHORT. Panel **RESOLVES IN** ngitung mundur pakai jam on-chain.
12. Abis expiry: **EXPIRED · AWAITING PYTH** — nunjukin harga Pyth terakhir + umurnya. Akun Pyth gratisan
    (sponsored) update **kira-kira sejam sekali** → tunggu sampai tulisannya *"after expiry, ready to resolve"*
    (**potong** bagian nunggu di video). Dengan kunci Hermes (belum ada) ini bakal beberapa detik.
13. Klik **RESOLVE FROM PYTH** (siapa pun boleh) → **RESOLVED · PYTH VERIFIED $X**: kontrak baca akun Pyth sendiri
    & ngunci harganya. Tombol pump/crash **nggak ada** di pasar prediksi — on-chain juga ditolak
    (`CreatorPriceOnPrediction`).
14. **CLOSE POSITION** → settle di harga terkunci.

### Angka panggung — semua dikunci tes

| Angka di layar | Nilai (P = harga Pyth XAU saat mulai) | Dikunci di |
|---|---|---|
| Est. fill / entry LONG 0.5 · 5× | +5.00% → P × 1.05 | `app/e2e-engine.mjs` A2 · litesvm `test_skew_premium_long_up_short_down` |
| Fill premium setelah open | +10.00% | `app/src/engine.ts` = on-chain (e2e-engine A2) |
| PnL setelah pump +25% dari entry | ≈ +125% → payout ≈ 1.125 SOL (− funding beberapa lamport) | e2e-engine A3 |
| Liq price · crash | entry × 0.85 · entry × 0.75 | e2e-engine A4 |
| Bounty likuidasi | 0.025 SOL | e2e-engine A4 |
| Putusan prediksi | harga Pyth, bukan karangan pembuat pasar | litesvm `test_resolve_ignores_creator_mark` · e2e 2/4 + 3/4 |

Contoh run 22 Okt 2026 (XAU dari clone Pyth devnet): P = $4,061.515 → entry $4,264.59 → payout 1.124999974 SOL.
Harga awal ngikut Pyth hari itu, jadi dolarnya berubah-ubah; persennya tetap.

---

## 3. Skrip video (≤3 mnt, kata-per-kata)

> Narasi **English** (juri global). `[LAYAR]` = yang keliatan, `[LAKU]` = yang lu klik. Total ~2:55 — ngomong
> pelan, biarin angka kebaca. ⛔ Hook bisa diganti cerita "user bernama" (keputusan tim yang masih kebuka).

**0:00–0:15 — HOOK**
[LAYAR: Perpetua, "THE MARKET NEVER CLOSES"]
> "Every perps exchange lists the same handful of markets. The hard part was never trading a market — it's
> creating one. Perpetua lets anyone launch a leveraged market on any Pyth price feed, in seconds."

**0:15–0:35 — IGNITE**
[LAKU: XAU/USD → PERP → IGNITE MARKET]
> "I pick a feed — gold — and ignite. That's it: a live, leveraged market exists on-chain. No orderbook, no
> listing committee, no market makers to court."

**0:35–1:15 — TRADE: SIZE MOVES THE PRICE**
[LAKU: tunjuk Est. fill +5% → OPEN LONG → tunjuk strip SKEW + FUNDING → pump +25%]
> "My counterparty is an automated liquidity pool. Look at the fill: five percent above the mark — my own order
> size moves my price; nobody types it in. Now the market is long-heavy, so funding starts ticking: longs pay
> shorts, a little more every second. The price moves, and my PnL updates live from on-chain state."
[LAYAR: PnL ijo ≈ +125%]

**1:15–1:30 — CLOSE + PAYOUT**
[LAKU: CLOSE POSITION → tunjuk tx]
> "Close, and the payout settles on-chain, straight to my wallet."

**1:30–1:55 — LIKUIDASI**
[LAKU: OPEN LONG → crash −25% → LIQUIDATE (demo)]
> "If the price crashes, this position is underwater — and liquidation is permissionless. Anyone can close it
> and earn the bounty. That's what protects the pool, with no central operator."

**1:55–2:40 — PREDIKSI: PYTH IS THE JUDGE** *(devnet)*
[LAKU: SOL/USD → PREDICTION · 1M → IGNITE PREDICTION → OPEN LONG → (potong nunggu) → RESOLVE FROM PYTH → CLOSE]
> "Same engine, with an expiry — and it's a prediction market: will SOL be higher in one minute? When it expires,
> anyone can press resolve. The contract reads Pyth's signed price account itself and locks the result. Not even
> me, the market's creator, can type it in — the program rejects it. Positions settle at the locked price.
> A market that lives for one minute only makes sense on a chain where every click is a real, fast, cheap
> transaction — that's why it's on Solana."

**2:40–2:55 — VISI + TUTUP**
[LAYAR: balik ke daftar feed / logo]
> "Perps and prediction markets — one engine. If a number has a Pyth feed, you can trade it, permissionlessly.
> **Perpetua. The market never closes.**"

**Jangan diucapin** (belum bener di kode / belum diukur): "the first price after expiry" (kontrak nerima
harga Pyth mana aja dalam 2 jam setelah expiry) · angka latency/kecepatan · "the only / the first" · "vAMM"
tanpa penjelasan · "every price comes from Pyth" (perp lokal pakai harga demo pembuat pasar).

---

## 4. Tips rekam
- Layar 1080p+, browser zoom 110–125%, kursor gede kalau bisa.
- Rekam per babak, gabung nanti — kalau satu babak nge-lag, ulang babak itu aja. Babak 4 = devnet; babak 1–3
  boleh lokal (topbar nunjukin `localnet`/`devnet` — biarin keliatan, jangan ngaku devnet kalau lokal).
- OBS / QuickTime / Loom oke. Mic jelas > kualitas gambar.
- Close-up 1–2 detik ke **Est. fill**, strip **FUNDING** yang lagi naik, **PnL**, dan tx — bukti "beneran on-chain".
- Versi Indonesia: narasi di atas gampang dialih — arti sama, tetep ≤3 mnt.

## 5. Kalau ngadat (recovery)
- **Tombol "Failed"** → baca status bar paling bawah; devnet: cek Phantom di jaringan Devnet & saldo cukup.
- **PnL nggak gerak** → normal di perp lokal: harga cuma gerak lewat **pump/crash**. Funding tetep jalan tiap detik.
- **Tombol pump/crash ilang** → by design: pasar prediksi, atau perp yang udah pernah dapet harga Pyth (override
  pembuat pasar pensiun on-chain — `CreatorPriceRetired`). Pakai pasar perp lokal yang fresh.
- **Likuidasi bilang "PositionHealthy"** → crash-nya kurang dalem buat leverage itu; crash lagi atau naikin leverage.
- **Resolve di lokal: "hasn't published a price after expiry"** → akun Pyth hasil clone itu BEKU (nggak pernah
  update) → ronde lokal nggak bakal ke-resolve. Babak resolve = devnet. ("No Pyth price account" = validator
  dinyalain tanpa internet → nggak ada clone.)
- **Resolve: "hasn't published a price after expiry yet"** → tunggu update Pyth berikutnya (~sejam), coba lagi.
- **Devnet lambat/mahal pas rekam babak 1–3** → pindah ke Jalur B (lokal), tampilannya identik.
- **Validator lokal error** → `pkill -f solana-test-validator` lalu `bash demo-local.sh` lagi.

## 6. Checklist submit
- [ ] Video ≤3 mnt ke-upload (publik: YouTube/Loom unlisted-public).
- [ ] Repo GitHub publik (commit pakai identitas **bre + temen**).
- [ ] README kebaca (wedge, arsitektur, cara jalanin, why-Solana).
- [ ] Link live/demo devnet kalau ada. Kalau nggak, video + repo cukup.
- [ ] Satu submission per tim. Cek Rules/Discord buat field wajib + aturan kode-AI + kode pra-lomba.
