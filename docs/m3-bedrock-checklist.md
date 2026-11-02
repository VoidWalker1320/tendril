# M3: Bedrock real-client checklist

The M3 gate needs a real Bedrock client: "joins, builds, chats, uses containers and fights". Bots (BotMark over NetherNet in CI) cover joining, moving and chat; this list covers the rest.

## Setup on the phone

1. In Termux: `mkdir -p ~/bedrock-test && cd ~/bedrock-test && termux-wake-lock && ~/tendril-m3 --accept-eula`
2. Wait for `Ready in ... ms`. The `Bedrock Edition:` line shows the address.
3. Minecraft (Bedrock 26.60 or 26.61, both protocol 2225, signed in to Xbox): Play → Servers → Add Server.
   - Same phone: address `127.0.0.1`, port `19132`.
   - Another device on the same Wi-Fi: the address from the `Bedrock Edition:` line, port `19132`.
4. Android may close Termux while Minecraft is in front on a 4 GB phone. If the server disappears, run it on another device or keep Minecraft's render distance low.

## Gate (all must work)

| # | Do | Expect |
| --- | --- | --- |
| 1 | Join | You spawn; your name shows as `.YourGamertag` (spaces become `_`). Console: a join line. |
| 2 | Place and break blocks | Blocks appear and disappear; broken blocks drop items. |
| 3 | Chat | Your message shows in game and in the Termux console. |
| 4 | Open a chest, move items in and out, close, reopen | Items stay where you put them. |
| 5 | Craft something at a crafting table | The result arrives in your inventory. |
| 6 | Hit a mob (or let a zombie hit you at night) | Damage, knockback, health changes. |
| 7 | Leave and join again | Same position and inventory (stable UUID). |

## Also check (section 8 parity)

| Do | Expect |
| --- | --- |
| Your skin | Shown to other players (needs a second device) |
| Type `/` | Command list with autocomplete |
| Ride a boat or minecart | You can get in, move, get out |
| `/linkaccount <your Java name> <code>` after a Java player ran `/linkaccount .YourGamertag` | Kicked with "Linked to ..."; on rejoin you are the Java player |
| Optional: put a `.mcpack` in `[resource_pack.bedrock]` of `pumpkin.toml` with `enabled = true`, rejoin | The client downloads the pack and shows it |

## Report back

For each row: works, partly (what happened) or fails (what you saw), plus any red lines in the Termux console. A screen recording of rows 2-6 is the best evidence.
