// Smoke test: panggil program yang ter-deploy pakai jalur client yang SAMA
// kayak frontend (Program + accountsPartial + PDA). Lawan validator lokal.
import { readFileSync } from 'fs'
import { homedir } from 'os'
import anchor from '@coral-xyz/anchor'
const { AnchorProvider, Program, BN, Wallet } = anchor
import { Connection, PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from '@solana/web3.js'

const RPC = 'http://127.0.0.1:8899'
const PYTH_SOL = 'ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d'

const idl = JSON.parse(readFileSync(new URL('./src/idl/perpetua.json', import.meta.url)))
const secret = JSON.parse(readFileSync(homedir() + '/.config/solana/id.json'))
const kp = Keypair.fromSecretKey(Uint8Array.from(secret))
const conn = new Connection(RPC, 'confirmed')
const provider = new AnchorProvider(conn, new Wallet(kp), { commitment: 'confirmed' })
const program = new Program(idl, provider)
const PID = new PublicKey(idl.address)

const feed = new PublicKey(Buffer.from(PYTH_SOL, 'hex'))
const market = PublicKey.findProgramAddressSync([Buffer.from('market'), feed.toBuffer()], PID)[0]
const vault = PublicKey.findProgramAddressSync([Buffer.from('vault'), market.toBuffer()], PID)[0]
const position = PublicKey.findProgramAddressSync(
  [Buffer.from('position'), market.toBuffer(), kp.publicKey.toBuffer()],
  PID,
)[0]

const bal = async () => (await conn.getBalance(kp.publicKey)) / LAMPORTS_PER_SOL
const get = (o, a, b) => (o[a] !== undefined ? o[a] : o[b])

console.log('wallet', kp.publicKey.toBase58(), '| balance', await bal())
console.log('program', PID.toBase58())

console.log('\n[1] create_market ($100, 0.5 SOL likuiditas)...')
await program.methods
  .createMarket(feed, new BN(100_000_000), new BN(0.5 * LAMPORTS_PER_SOL))
  .accountsPartial({ authority: kp.publicKey, market, vault, systemProgram: SystemProgram.programId })
  .rpc()
let m = await program.account.market.fetch(market)
console.log('    OK markPrice=', get(m, 'markPrice', 'mark_price').toString(), 'authority=', get(m, 'authority', 'authority').toBase58())

console.log('\n[2] open_position LONG 5x, jaminan 0.5 SOL...')
await program.methods
  .openPosition(new BN(0.5 * LAMPORTS_PER_SOL), new BN(5), true)
  .accountsPartial({ trader: kp.publicKey, market, vault, position, systemProgram: SystemProgram.programId })
  .rpc()
let p = await program.account.position.fetch(position)
console.log('    OK size=', get(p, 'size', 'size').toString(), 'entry=', get(p, 'entryPrice', 'entry_price').toString(), 'isLong=', get(p, 'isLong', 'is_long'))

console.log('\n[3] set_mark_price $110 (+10%)...')
await program.methods.setMarkPrice(new BN(110_000_000)).accountsPartial({ market, authority: kp.publicKey }).rpc()

const before = await bal()
console.log('\n[4] close_position (balance sebelum', before, ')...')
await program.methods
  .closePosition()
  .accountsPartial({ trader: kp.publicKey, market, vault, position })
  .rpc()
const after = await bal()
console.log('    balance sesudah', after, '=> delta', (after - before).toFixed(4), 'SOL (harapan ~+0.75: jaminan 0.5 + PnL 0.25)')

console.log('\nSMOKE OK — jalur client frontend TERBUKTI jalan lawan program ter-deploy.')
