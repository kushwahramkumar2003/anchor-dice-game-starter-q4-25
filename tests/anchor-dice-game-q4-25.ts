import * as anchor from "@coral-xyz/anchor";
import { Program, web3, BN } from "@coral-xyz/anchor";
import { expect } from "chai";

import { AnchorDiceGameQ425 } from "../target/types/anchor_dice_game_q4_25";

describe("anchor-dice-game-q4-25", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace
    .anchorDiceGameQ425 as Program<AnchorDiceGameQ425>;

  const player = web3.Keypair.generate();
  const house = web3.Keypair.generate();

  const SYSTEM_PROGRAM = web3.SystemProgram.programId;
  const ED25519_PROGRAM = new web3.PublicKey(
    "Ed25519SigVerify111111111111111111111111111"
  );
  const INSTRUCTION_SYSVAR = new web3.PublicKey(
    "Sysvar1nstructions1111111111111111111111111"
  );

  const airdrop = async (pubkey: web3.PublicKey, sol = 20) => {
    const sig = await provider.connection.requestAirdrop(
      pubkey,
      sol * web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig, "confirmed");
  };

  const vaultPda = () =>
    web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), house.publicKey.toBuffer()],
      program.programId
    )[0];

  const betPda = (vault: web3.PublicKey, seed: bigint) => {
    const seedBuf = Buffer.alloc(16);
    seedBuf.writeBigUInt64LE(seed & ((1n << 64n) - 1n), 0);
    seedBuf.writeBigUInt64LE(seed >> 64n, 8);
    return web3.PublicKey.findProgramAddressSync(
      [Buffer.from("bet"), vault.toBuffer(), seedBuf],
      program.programId
    )[0];
  };

  const initializeVault = async (amountLamports: number) => {
    const vault = vaultPda();
    await program.methods
      .initialize(new BN(amountLamports))
      .accounts({
        house: house.publicKey,
        vault,
        systemProgram: SYSTEM_PROGRAM,
      })
      .signers([house])
      .rpc();
    return vault;
  };

  const placeBet = async (seed: bigint, roll: number, amountLamports: number) => {
    const vault = vaultPda();
    const bet = betPda(vault, seed);

    await program.methods
      .placeBet(new BN(seed.toString()), roll, new BN(amountLamports))
      .accounts({
        player: player.publicKey,
        house: house.publicKey,
        vault,
        bet,
        systemProgram: SYSTEM_PROGRAM,
      })
      .signers([player])
      .rpc();

    return { vault, bet };
  };

  const resolveBetWithSignature = async (
    bet: web3.PublicKey,
    signedMessage: Buffer,
    signer: web3.Keypair = house
  ) => {
    const vault = vaultPda();

    const edIx = web3.Ed25519Program.createInstructionWithPrivateKey({
      privateKey: signer.secretKey,
      message: signedMessage,
    });

    return await program.methods
      .resolveBet(signedMessage)
      .accounts({
        player: player.publicKey,
        house: house.publicKey,
        vault,
        bet,
        instructionSysvar: INSTRUCTION_SYSVAR,
        systemProgram: SYSTEM_PROGRAM,
      })
      .preInstructions([edIx])
      .signers([player])
      .rpc();
  };

  before(async () => {
    await airdrop(provider.wallet.publicKey, 5);
    await airdrop(player.publicKey, 20);
    await airdrop(house.publicKey, 20);
  });

  it("initializes house vault and accepts a valid bet", async () => {
    const vault = await initializeVault(5 * web3.LAMPORTS_PER_SOL);

    const seed = 11n;
    const { bet } = await placeBet(seed, 60, 200_000_000);
    const betAccount = await program.account.bet.fetch(bet);

    expect(betAccount.player.toBase58()).eq(player.publicKey.toBase58());
    expect(betAccount.seed.toString()).eq(seed.toString());
    expect(betAccount.roll).eq(60);
    expect(betAccount.amount.toString()).eq("200000000");

    const vaultBal = await provider.connection.getBalance(vault);
    expect(vaultBal).greaterThan(5 * web3.LAMPORTS_PER_SOL);
  });

  it("resolves a winning bet using ed25519 instruction introspection", async () => {
    const seed = 22n;
    const amount = 100_000_000;
    const roll = 90;

    const { bet } = await placeBet(seed, roll, amount);

    const beforePlayer = await provider.connection.getBalance(player.publicKey);

    // 42 % 100 = 42 <= 90, so this is a win.
    await resolveBetWithSignature(bet, Buffer.from([42]));

    const afterPlayer = await provider.connection.getBalance(player.publicKey);
    expect(afterPlayer).greaterThan(beforePlayer);

    const betInfo = await provider.connection.getAccountInfo(bet);
    expect(betInfo).eq(null);
  });

  it("fails resolve when ed25519 signer does not match house", async () => {
    const seed = 33n;
    const { bet } = await placeBet(seed, 50, 100_000_000);

    let failed = false;
    try {
      await resolveBetWithSignature(bet, Buffer.from([7]), web3.Keypair.generate());
    } catch (e) {
      failed = true;
      expect(`${e}`).includes("Ed25519 Pubkey Error");
    }

    expect(failed).eq(true);
  });

  it("fails resolve when instruction introspection is missing", async () => {
    const seed = 44n;
    const { bet } = await placeBet(seed, 50, 100_000_000);
    const vault = vaultPda();

    let failed = false;
    try {
      await program.methods
        .resolveBet(Buffer.from([9]))
        .accounts({
          player: player.publicKey,
          house: house.publicKey,
          vault,
          bet,
          instructionSysvar: INSTRUCTION_SYSVAR,
          systemProgram: SYSTEM_PROGRAM,
        })
        .signers([player])
        .rpc();
    } catch (e) {
      failed = true;
      expect(`${e}`).includes("Ed25519 Header Error");
    }

    expect(failed).eq(true);
  });
});
