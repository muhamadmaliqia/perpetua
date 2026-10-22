import { AnchorProvider, Program, BN, type Idl } from '@coral-xyz/anchor'
import { Connection, PublicKey, Keypair } from '@solana/web3.js'
import idl from './idl/perpetua.json'

export const PROGRAM_ID = new PublicKey('GTaf7icvPHNzDMfd1gt8eN5p3fKERRwPaenfWKjGhaGP')
export const PRICE_ONE = 1_000_000 // harga 6 desimal
export const LAMPORTS = 1_000_000_000
// Kedalaman skew (virtual AMM) default demo: size 2.5 SOL -> premium ~5% (harga gerak karena ukuran).
export const SKEW_SCALE = 25 * LAMPORTS

// Feed kurasi. `pythId` = Pyth price-feed id (32 byte hex) -> dipakai jadi oracle_feed on-chain
// (feed = PublicKey dari byte-nya). Identitas oracle tiap pasar = feed Pyth ini.
// Harga awal pasar = harga Pyth TERAKHIR dari akun sponsored di cluster ini (devnet: XAU/SOL/BTC/ETH; localnet:
// di-clone demo-local.sh). `fallback` (USD) cuma kepake kalau akunnya nggak ada (US500 di devnet, localnet offline)
// — angkanya = harga Pyth devnet 22 Okt 2026 biar nggak jauh dari dunia nyata. (Hermes digembok API-key sejak
// 26 Agu 2026 -> nggak ada keeper live; mark perp tanpa harga Pyth digerakin tombol pump/crash demo.)
// Feed lintas kelas aset — metal (emas) + indeks saham + crypto — nunjukin "pasar leverage buat
// ANGKA APA PUN": permissionless creation x feed non-crypto (yg launcher SPL nggak nyampe). id = Pyth ASLI.
export const FEEDS = [
  { symbol: 'XAU/USD', pythId: '765d2ba906dbc32ca17cc11f5310a89e9ee1f6420508c63861f2f8ba4ee34bb2', fallback: 4122.82 },
  // US500: no sponsored Pyth account on devnet -> fallback is an unverified reference level.
  { symbol: 'US500', pythId: 'e266cf0bf9a358bb19c4269f59b3d2e6258ecda5b0707e758f00c087f4baa3e9', fallback: 6000 },
  { symbol: 'SOL/USD', pythId: 'ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d', fallback: 105.82 },
  { symbol: 'BTC/USD', pythId: 'e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43', fallback: 82425.42 },
  { symbol: 'ETH/USD', pythId: 'ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace', fallback: 2434.72 },
]

export function feedPubkey(pythId: string): PublicKey {
  return new PublicKey(Buffer.from(pythId, 'hex'))
}

// F3: jenis pasar (sama dgn constants.rs).
export const MARKET_KIND_PERP = 0
export const MARKET_KIND_PREDICTION = 1

// F2: akun harga Pyth "sponsored" (PriceUpdateV2, di-update Pyth sendiri) = PDA program push-oracle,
// seed [shard u16 LE, feed_id]. Terukur 14 Okt 2026: SOL/USD -> 7UVi…; di devnet ada buat XAU/SOL/BTC/ETH
// (US500 belum), update ~sejam sekali. Kontrak baca akun ini sendiri buat resolve (owner/Full/feed dicek on-chain).
const PYTH_PUSH_ORACLE = new PublicKey('pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT')
export function pythPriceAccount(pythId: string, shard = 0): PublicKey {
  const s = Buffer.alloc(2)
  s.writeUInt16LE(shard)
  return PublicKey.findProgramAddressSync([s, Buffer.from(pythId, 'hex')], PYTH_PUSH_ORACLE)[0]
}

// Baca harga + publish_time dari data akun PriceUpdateV2 (layout sama dgn programs/perpetua/src/pyth.rs).
// null kalau bukan verifikasi Full / data kependekan. Cuma buat tampilan — keputusan tetap di kontrak.
export function decodePythPrice(data: Uint8Array): { price6: number; publishTime: number } | null {
  if (data.length < 41 + 60 || data[40] !== 1) return null
  const b = Buffer.from(data)
  const o = 41
  const price = Number(b.readBigInt64LE(o + 32))
  const expo = b.readInt32LE(o + 48)
  const publishTime = Number(b.readBigInt64LE(o + 52))
  return { price6: Math.round(price * 10 ** (expo + 6)), publishTime }
}

// Slot pasar = ["market", feed, expiry_ts i64 LE]: perp = expiry 0 (satu per feed), prediction = satu per jatuh tempo.
export function marketPda(feed: PublicKey, expiryTs = 0): PublicKey {
  const e = Buffer.alloc(8)
  e.writeBigInt64LE(BigInt(expiryTs))
  return PublicKey.findProgramAddressSync([Buffer.from('market'), feed.toBuffer(), e], PROGRAM_ID)[0]
}
export function vaultPda(market: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([Buffer.from('vault'), market.toBuffer()], PROGRAM_ID)[0]
}
export function positionPda(market: PublicKey, owner: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('position'), market.toBuffer(), owner.toBuffer()],
    PROGRAM_ID,
  )[0]
}

export function getProgram(connection: Connection, wallet: any): Program {
  const provider = new AnchorProvider(connection, wallet, { commitment: 'confirmed' })
  return new Program(idl as Idl, provider)
}

// baca field IDL yg mungkin camelCase atau snake_case (aman lintas versi anchor).
export function pick(o: any, camel: string, snake: string): any {
  return o?.[camel] !== undefined ? o[camel] : o?.[snake]
}

// Localnet-only burner wallet: auto-signs, no Phantom + no faucet needed. Persisted in localStorage
// so the same address survives refreshes. NEVER used on devnet (there the real Phantom wallet is used).
export function getBurnerKeypair(): Keypair {
  const KEY = 'perpetua-localnet-burner-v1'
  try {
    const saved = localStorage.getItem(KEY)
    if (saved) return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(saved)))
  } catch {
    /* fall through to a fresh key */
  }
  const kp = Keypair.generate()
  try {
    localStorage.setItem(KEY, JSON.stringify(Array.from(kp.secretKey)))
  } catch {
    /* non-persistent is fine for a demo */
  }
  return kp
}

export function makeBurnerWallet(kp: Keypair) {
  const sign = (tx: any) => {
    if (typeof tx.partialSign === 'function') tx.partialSign(kp)
    else if (typeof tx.sign === 'function') tx.sign([kp])
    return tx
  }
  return {
    publicKey: kp.publicKey,
    payer: kp,
    signTransaction: async (tx: any) => sign(tx),
    signAllTransactions: async (txs: any[]) => txs.map(sign),
  }
}

export { BN }
