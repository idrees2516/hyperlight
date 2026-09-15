# Zoom verification (2x) of claimed defects

## arb1_tradetape

1) **First 6 rows:**
- 09:29:30 | 2475.26000000 | 0.4042 | $1.0K | BN
- 09:29:30 | 2473.88 | 0.0006 | $1 | LT
- 09:29:30 | 2474.7 | 0.0128 | $32 | HL
- 09:29:30 | 2474.16 | 0.601 | $1.5K | CB
- 09:29:30 | 2474.16 | 0.8718 | $2.2K | CB
- 09:29:30 | 2474.16 | 0.9572 | $2.4K | CB

2) **Overlap:** Each value is cleanly inside its column with clear whitespace. The PRICE and SIZE columns are well-separated; there is no text collision, overlapping, or bleeding between them on any visible row.

3) **Clipping:** Yes, the very last row (starting with "00:20:28") at the bottom of the Trade Tape panel is visibly clipped/cut off horizontally, which appears to be a rendering or layout issue rather than a normal scroll state (the text is sliced mid-character).

4) **Decimal Precision:** No, the decimal precisions are not consistent. 
   - Most prices use 2 decimal places (e.g., 2473.88, 2474.7, 2474.16).
   - **Outliers (high precision):** Row 1 uses 8 decimal places (`2475.26000000`), Row 8 uses 7 decimal places (`2474.16 1.21241148` - wait, that's the size, price is `2474.16`), Row 15 uses 8 decimal places (`2475.27000000`). Actually, looking closely:
     - Row 1 Price: `2475.26000000` (8 decimals)
     - Row 8 Price: `2474.16` (2 decimals), but Size is `1.21241148`
     - Row 14 Price: `2475.27000000` (8 decimals)
     - Row 17 Size: `0.00055159` (8 decimals)
   - So the **Price outliers** are rows 1 and 14 with 8 decimal places, while most others have 1 or 2.

Verdict: **NO OVERLAP**, **CLIPPING CONFIRMED**

---

## arb1_venuebooks

Based on the provided image of the trading terminal's "VENUE BOOKS" grid:

1) **Kraken tile:** The exact BID SZ value is **45.58828129**. It is **clipped/cut off** at the right edge of its cell; the final digit '9' is partially truncated by the border, and it does not overlap the LEVELS column.
2) **Coinbase tile:** The BID SZ is **0.25521148**. It is **clipped**; the final digit '8' is cut off by the right edge of the cell.
3) **OKX tile:** The placeholder text for BID/ASK/BID SZ is **"waiting..."** (displayed in a dim grey font). Yes, there is an orange **"NO FEED"** badge.
4) **Gate tile:** The values for BID, ASK, and BID SZ are all dashes (**–**), and LEVELS shows **0**. Yes, there is a msg/s value (**14.3**) despite the missing price data.
5) **Other tiles:** No other tiles show clipping or misalignment. All other numerical values fit within their respective columns.

KRAKEN CLIPPED: yes
COINBASE CLIPPED: yes
GATE INCONSISTENCY: yes
OTHER DEFECTS: none

---

## arb3_book_sum

Based on the visual analysis of the consolidated order book:

1) **Transcription of 3 rows with the largest SIZE or SUM values:**
*   **Row A:** PRICE: `100.61` | SIZE: `619.8193618` | SUM: `2.44K` (Largest SIZE value)
*   **Row B:** PRICE: `100.6` | SIZE: `639.20727793` | SUM: `1.39K` (Second largest SIZE)
*   **Row C:** PRICE: `100.599` | SIZE: `119.0417` | SUM: `749.37` (Largest SUM value)

2) **Overflow Status:** All numeric values fit cleanly within their respective columns. The font size and column widths are well-proportioned, ensuring that even long decimal values in the SIZE column do not encroach upon the SUM or VENUE columns.

3) **Venue Badge Alignment:** The venue badges on the right form a clean, vertically aligned column. They are consistently right-aligned, with no noticeable zigzagging or misalignment between rows.

4) **Clipping Status:** There is no text clipped at the right edge of the panel. All venue badges are fully visible within the container.

**Verdict:** NO OVERFLOW; BADGES MISALIGNED: no

---

## arb4_tradetape_binance

1) **First 6 rows:**
- 09:29:42 | 100.586 | 0.01 | $1 | LT
- 09:29:41 | 100.6 | 4.6195 | $465 | KR
- 09:29:41 | 100.6 | 8.31018 | $836 | KR
- 09:29:41 | 100.64 | 2.963268 | $298 | OK
- 09:29:41 | 100.585 | 0.994 | $100 | LT
- 09:29:40 | 100.588 | 1 | $101 | B5

**Last 3 visible rows:**
- 09:29:40 | 100.64 | 17.857 | $1.8K | GT
- 09:29:40 | 100.61 | 0.04872179 | $5 | KR
- 00:20:20 | 100.588 | 1 | $101 | RC

2) **Yes**, the very last row (timestamp `00:20:20`) is partially cut off at the bottom edge of the panel container, and its text appears slightly compressed/distorted compared to the rows above it, which looks like a rendering/clipping bug rather than a standard scroll fade.

3) **No**, all columns (TIME, PRICE, SIZE, VENUE) are clearly separated with consistent alignment; no values overlap.

4) **Distinct venue tags:** LT, KR, OK, B5, CB, GT, RC

**Verdict:** CLIPPING CONFIRMED; NO OVERLAP

---

## arb1_header

Based on the image, here is the exact transcription and analysis of the badges:

**Exact Badge Transcriptions:**
1.  **HYPERLIQUID LIVE**
2.  **LIGHTER LIVE**
3.  **BINANCE LIVE**
4.  **BYBIT LIVE**
5.  **OKX CONNECTING**
6.  **KRAKEN LIVE**
7.  **COINBASE LIVE**
8.  **BITSTAMP LIVE**
9.  **GATE LIVE**

**Answers to your questions:**

*   **Total Badges:** There are **9** venue connection badges.
*   **"Live" Count:** **8** of the badges say "LIVE".
*   **Other Status Pills:** Yes, there is one additional status pill in the top-right corner that reads: **WS OPEN** (indicating the WebSocket connection status).
*   **Truncation/Wrapping:** No, none of the badge text is truncated or wrapped. Every badge displays its venue name and status on a single line within a fully visible pill shape.

---

