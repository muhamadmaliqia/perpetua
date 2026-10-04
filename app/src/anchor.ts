import { AnchorProvider, Program, BN, type Idl } from '@coral-xyz/anchor'
import { Connection, PublicKey, Keypair } from '@solana/web3.js'
import idl from './idl/perpetua.json'

export const PROGRAM_ID = new PublicKey('GTaf7icvPHNzDMfd1gt8eN5p3fKERRwPaenfWKjGhaGP')
export const PRICE_ONE = 1_000_000 // harga 6 desimal
export const LAMPORTS = 1_000_000_000

// Feed kurasi. `pythId` = Pyth price-feed id (32 byte hex) -> dipakai jadi oracle_feed on-chain
// (feed = PublicKey dari byte-nya). Identitas oracle tiap pasar = feed Pyth ini.
// `fallback` = harga awal pasar (USD). (Pyth public Hermes digembok API-key sejak 26 Agu 2026, jadi
// demo nggak fetch harga live; mark digerakin lewat set_mark_price / tombol pump-crash.)
export const FEEDS = [
  { symbol: 'SOL/USD', pythId: 'ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d', fallback: 150 },
  { symbol: 'BTC/USD', pythId: 'e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43', fallback: 95000 },
  { symbol: 'ETH/USD', pythId: 'ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace', fallback: 3500 },
]

export function feedPubkey(pythId: string): PublicKey {
  return new PublicKey(Buffer.from(pythId, 'hex'))
}

export function marketPda(feed: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([Buffer.from('market'), feed.toBuffer()], PROGRAM_ID)[0]
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
