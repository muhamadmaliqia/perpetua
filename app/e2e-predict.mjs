// E2E prediction market lawan validator lokal, pakai jalur client yang SAMA kayak frontend
// (createMarket kind=1 + expiry on-chain, openPosition, resolveMarket + priceUpdate = akun Pyth feed).
//   node e2e-predict.mjs <feedHex> <harga awal USD> <expect: awaiting|resolved> [harga resolusi USD]
// awaiting = akun Pyth ada tapi terbit SEBELUM expiry (mis. clone beku dari devnet) -> resolve DITOLAK.
// resolved = akun Pyth terbit SETELAH expiry (fixture e2e) -> resolve LOLOS, close settle di harga itu.
import { readFileSync } from 'fs'
import { homedir } from 'os'
import anchor from '@coral-xyz/anchor'
const { AnchorProvider, Program, BN, Wallet } = anchor
import { Connection, PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from '@solana/web3.js'

const [feedHex, startUsd, expect, resolveUsd] = process.argv.slice(2)
const RPC = 'http://127.0.0.1:8899'
const EXPIRY_IN = 12 // detik

const idl = JSON.parse(readFileSync(new URL('./src/idl/perpetua.json', import.meta.url)))
const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(homedir() + '/.config/solana/id.json'))))
const conn = new Connection(RPC, 'confirmed')
const program = new Program(idl, new AnchorProvider(conn, new Wallet(kp), { commitment: 'confirmed' }))
const PID = new PublicKey(idl.address)

const feed = new PublicKey(Buffer.from(feedHex, 'hex'))
const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, PID)[0]
const market = pda([Buffer.from('market'), feed.toBuffer()])
const vault = pda([Buffer.from('vault'), market.toBuffer()])
const position = pda([Buffer.from('position'), market.toBuffer(), kp.publicKey.toBuffer()])
// Akun harga Pyth sponsored = PDA push-oracle [shard 0 u16 LE, feed_id] (sama dgn anchor.ts pythPriceAccount).
const priceUpdate = PublicKey.findProgramAddressSync(
  [Buffer.alloc(2), Buffer.from(feedHex, 'hex')],
  new PublicKey('pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT'),
)[0]

const chainNow = async () => conn.getBlockTime(await conn.getSlot('confirmed'))
const bal = async () => (await conn.getBalance(kp.publicKey)) / LAMPORTS_PER_SOL
const usd6 = (x) => new BN(Math.round(Number(x) * 1e6))
const SKEW_BIG = new BN(100_000_000).mul(new BN(LAMPORTS_PER_SOL)) // premium ~0 biar angka gampang dicek
const fail = (msg) => {
  console.log('E2E GAGAL:', msg)
  process.exit(1)
}

const pyAcc = await conn.getAccountInfo(priceUpdate)
console.log(`feed ${feedHex.slice(0, 8)}… | akun Pyth ${priceUpdate.toBase58()} ${pyAcc ? 'ADA (owner ' + pyAcc.owner.toBase58().slice(0, 8) + '…)' : 'TIDAK ADA'}`)

const expiry = (await chainNow()) + EXPIRY_IN
console.log(`[1] create_market PREDICTION @ $${startUsd}, expiry +${EXPIRY_IN}s (${expiry})`)
await program.methods
  .createMarket(feed, usd6(startUsd), new BN(2 * LAMPORTS_PER_SOL), SKEW_BIG, 1, new BN(expiry))
  .accountsPartial({ authority: kp.publicKey, market, vault, systemProgram: SystemProgram.programId })
  .rpc()

console.log('[2] open_position LONG 5x, jaminan 0.5 SOL')
await program.methods
  .openPosition(new BN(0.5 * LAMPORTS_PER_SOL), new BN(5), true)
  .accountsPartial({ trader: kp.publicKey, market, vault, position, systemProgram: SystemProgram.programId })
  .rpc()
const entry = Number((await program.account.position.fetch(position)).entryPrice.toString())
console.log('    entry', entry / 1e6)

process.stdout.write('[3] nunggu expiry on-chain')
while ((await chainNow()) < expiry) {
  process.stdout.write('.')
  await new Promise((r) => setTimeout(r, 1000))
}
console.log(' lewat.')

console.log('[4] resolve_market (resolver = siapa pun; kontrak baca akun Pyth sendiri)')
let resolveErr = null
try {
  await program.methods.resolveMarket().accountsPartial({ resolver: kp.publicKey, market, priceUpdate }).rpc()
} catch (e) {
  resolveErr = e?.message ?? String(e)
}
const m = await program.account.market.fetch(market)

if (expect === 'awaiting') {
  if (!resolveErr?.includes('OraclePriceBeforeExpiry')) fail('mestinya ditolak OraclePriceBeforeExpiry, dapet: ' + (resolveErr ?? 'SUKSES'))
  if (m.resolved) fail('market nggak boleh resolved')
  console.log('    DITOLAK sesuai harapan: OraclePriceBeforeExpiry (harga Pyth di akun terbit sebelum expiry)')
  console.log('\nE2E OK (awaiting) — kontrak nolak harga Pyth yang terbit sebelum expiry.')
  process.exit(0)
}

if (resolveErr) fail('resolve mestinya lolos, dapet: ' + resolveErr)
const res6 = Number(m.resolutionPrice.toString())
console.log(`    RESOLVED @ $${res6 / 1e6} (lastOracleTs ${m.lastOracleTs.toString()})`)
if (res6 !== Number(usd6(resolveUsd).toString())) fail(`harga resolusi mestinya $${resolveUsd}`)

const before = await bal()
console.log('[5] close_position -> settle di harga terkunci')
await program.methods.closePosition().accountsPartial({ trader: kp.publicKey, market, vault, position }).rpc()
const delta = (await bal()) - before
const expected = 0.5 + (2.5 * (res6 - entry)) / entry
console.log(`    delta ${delta.toFixed(4)} SOL (harapan ~${expected.toFixed(4)} = jaminan 0.5 + PnL, + refund rent posisi - fee)`)
if (Math.abs(delta - expected) > 0.01) fail('payout nggak sesuai harga resolusi')
console.log('\nE2E OK (resolved) — resolve baca Pyth on-chain, close settle di harga terkunci.')
