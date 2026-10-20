// E2E FIXTURE (BUKAN data Pyth asli): bikin file akun PriceUpdateV2 SINTETIS buat dipasang ke
// solana-test-validator lewat `--account`. Tujuannya cuma ngetes kabel client -> kontrak buat jalur
// resolve SUKSES di validator beneran (data Pyth asli yang seger butuh kunci Hermes). Jangan dipakai
// buat demo/video. Layout sama dgn programs/perpetua/src/pyth.rs.
//   node e2e-pyth-fixture.mjs <feedHex> <harga USD> <detik dari sekarang> <file keluaran>
import { writeFileSync } from 'fs'
import { createHash } from 'crypto'
import { PublicKey } from '@solana/web3.js'

const [feedHex, usd, inSec, out] = process.argv.slice(2)
const PUSH_ORACLE = new PublicKey('pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT')
const RECEIVER = 'rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ'
const shard = Buffer.alloc(2)
const address = PublicKey.findProgramAddressSync([shard, Buffer.from(feedHex, 'hex')], PUSH_ORACLE)[0]

const price = BigInt(Math.round(Number(usd) * 1e8)) // expo -8
const publishTime = BigInt(Math.floor(Date.now() / 1000) + Number(inSec))
const d = Buffer.alloc(134)
createHash('sha256').update('account:PriceUpdateV2').digest().copy(d, 0, 0, 8)
d[40] = 1 // verifikasi Full
Buffer.from(feedHex, 'hex').copy(d, 41)
d.writeBigInt64LE(price, 73)
d.writeBigUInt64LE(price / 2000n, 81) // conf 0.05%
d.writeInt32LE(-8, 89)
d.writeBigInt64LE(publishTime, 93)
d.writeBigInt64LE(publishTime - 1n, 101)
d.writeBigInt64LE(price, 109)
d.writeBigUInt64LE(price / 2000n, 117)

writeFileSync(
  out,
  JSON.stringify({
    pubkey: address.toBase58(),
    account: { lamports: 1_000_000_000, data: [d.toString('base64'), 'base64'], owner: RECEIVER, executable: false, rentEpoch: 0, space: 134 },
  }),
)
console.log(address.toBase58(), 'publish_time', publishTime.toString())
