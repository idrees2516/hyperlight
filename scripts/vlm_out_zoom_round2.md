# Round-2 zoom verification (tight crops, neutral prompts)

## arb2_kraken_tile_3x

1) BID=100.9, ASK=100.91, BID SZ=118.353918, LEVELS=207
2) 9 digits; the last digit (8) is fully drawn.
3) The gap is roughly 4–5 characters wide.
4) SOL-USD
5) No text is cut off, overlapping, or touching a border.

BID SZ LAST DIGIT: complete
GAP BID SZ->LEVELS: wide
OTHER CLIPPING: none

---

## arb2_coinbase_tile_3x

Based on the provided image of the Coinbase order-book tile, here are the precise transcriptions and observations:

1) **Full values row:**
BID=**100.9**, ASK=**100.91**, BID SZ=**91.918631**, LEVELS=**2**

2) **BID SZ digit count and last glyph status:**
The BID SZ value **91.918631** contains **8 digits**. The last digit (**1**) is a **complete glyph**; it is fully drawn and not sliced at the right edge.

3) **Whitespace gap description:**
There is a **wide** whitespace gap between the end of the BID SZ value ("91.918631") and the start of the LEVELS value ("2"). This gap is significantly larger than the spacing between individual characters within the numbers.

4) **Small bottom line text:**
The bottom line says exactly: **SOL-USD**

5) **Clipping/Overlapping analysis:**
No. There is no text cut off, overlapping, or touching any borders in this tile. All elements (header, table headers, data row, and bottom label) are fully contained within the dark background area with clear padding.

---
**Verdict lines:**
BID SZ LAST DIGIT: complete
GAP BID SZ->LEVELS: wide
OTHER CLIPPING: none

---

## arb1_gate_tile_3x

Based on the image provided, here is the transcription and analysis of the "Gate" venue tile:

1)  **Transcribed Values:**
    *   **BID =** 2478.41
    *   **ASK =** 2478.42
    *   **BID SZ =** 5.9006
    *   **LEVELS =** 40

2)  The BID/ASK values are **real numbers** (specifically, they are price values).

3)  Yes, **LEVELS is a number greater than zero** (it is 40).

4)  No, the literal word "**waiting**" does not appear anywhere in this tile.

5)  No, there is no text clipped or overlapping; the layout is clean.

GATE DATA: real; LEVELS>0: yes; WAITING TEXT: not-found; CLIPPING: none

---

## arb1_tape_bottom_3x

**1) Transcription of the last 3 rows of the tape:**
- Row 1 (3rd from bottom): `TIME` | `PRICE` | `SIZE` | `NOTIONAL` | `LT`
- Row 2 (2nd from bottom): `TIME` | `PRICE` | `SIZE` | `NOTIONAL` | `LT`
- Row 3 (Bottom-most): `TIME` | `PRICE` | `SIZE` | `NOTIONAL` | `LT`

*(Note: The image provided shows a "Price Alerts" panel to the right of a vertical list of "LT" badges, which appears to be the rightmost column/venue indicator of a trade tape. The specific time, price, and size values are not legible in this crop; only the repeating "LT" venue badges are clearly visible on the left edge of the frame.)*

**2) Bottom-most tape row status:**
The bottom-most visible row (the 5th "LT" badge from the top in the left column) is **fully drawn** with complete character height and is not sliced by any bottom edge within this crop.

**3) Padding below last row:**
There is **visible empty space / padding** between the last fully visible "LT" badge and the bottom edge of the image/crop area.

**4) Content below the tape panel:**
Below the vertical column of "LT" badges (the tape's venue column), there is the top edge of another UI element beginning to appear—specifically, the upper-left rounded corner of what looks like an orange or amber-colored button or status indicator.

---
**BOTTOM ROW:** fully-visible
**PADDING BELOW LAST ROW:** present

---

## arb4_binance_tape_2x

Based on the visual evidence in the provided image, here is the transcription and analysis of the "Trade Tape" panel:

**1) Transcription of the PRICE column (first 10 rows):**
1.  `100.95`
2.  `100.95`
3.  `100.95`
4.  `100.95`
5.  `100.95`
6.  `100.95`
7.  `100.95`
8.  `100.95`
9.  `100.95`
10. `100.95`

**2) Analysis of decimal places and trailing zeros:**
No. None of the price values contain more than 4 decimal places, nor do they display long trailing zeros (e.g., `100.95000000`). All visible prices are formatted to exactly two decimal places.

**3) Column overlap:**
No. The columns for TIME (grey), PRICE (green), SIZE (white), and VENUE (yellow badge) are clearly separated by spacing and do not overlap.

***

TRAILING-ZERO PRICES: not-found; LONGEST PRICE STRING: 100.95

---

## arb1_header_badges_2x

Based on the image provided, here are the details for the venue connection badges:

**1) Transcription of every badge:**
*   HYPERLIQUID LIVE
*   LIGHTER LIVE
*   BINANCE LIVE
*   BYBIT LIVE
*   OKX LIVE
*   KRAKEN LIVE
*   COINBASE LIVE
*   BITSTAMP LIVE
*   GATE LIVE

**2) Counts:**
There are **9** badges in total, and **9** of them say "live".

**3) OKX Badge Text:**
The OKX badge says exactly: **OKX LIVE**

**4) Truncation/Wrapping/Clipping:**
No. All badge text is fully visible, single-line, and completely contained within their respective rounded rectangular borders.

---
BADGE COUNT: 9; LIVE COUNT: 9; OKX STATUS: OKX LIVE

---

