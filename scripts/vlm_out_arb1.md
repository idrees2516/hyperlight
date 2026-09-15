# VLM review: download/arb1_eth_full.png (1440x3371)

## FULL-PAGE PASS

Here is the structured QA review of the provided screenshot.

### 1) HEADER VENUE BADGES
*   **Hyperliquid:** LIVE
*   **Lighter:** LIVE
*   **Binance:** LIVE
*   **Bybit:** LIVE
*   **OKX:** CONNECTING
*   **Kraken:** LIVE
*   **Coinbase:** LIVE
*   **Bitstamp:** LIVE
*   **Gate:** LIVE

**Total Count:** 9 badges.
**Live Count:** 8 (OKX is "CONNECTING").
**Set Verification:** All 9 expected venues are present. No missing or extra badges.

---

### 2) CROSS-VENUE ARBITRAGE ENGINE CARD
*   **Card Existence:** PRESENT. Titled "Cross-Venue Arbitrage Engine" with an "ARMED" status indicator.
*   **Stat Tiles:**
    *   **Net P&L:** PRESENT. Value: `$0.00` (paper).
    *   **Fills:** PRESENT. Value: `0 / 0 exp` (filled / expired).
    *   **Record:** PRESENT. Value: `0W-0L` (win-loss).
    *   **Best Edge:** PRESENT. Value: `0 bps`.
    *   **Avg Edge:** PRESENT. Value: `0 bps`.
    *   **Open Opps:** PRESENT. Value: `0` (live routes).
*   **Live Opportunities Table:**
    *   **Status:** PRESENT.
    *   **Headers:** MARKET, ROUTE (BUY → SELL), NOTIONAL, NET BPS, PROFIT.
    *   **Row Content:** Contains a single centered message: "scanning 9 venues... no route above the threshold right now".
*   **Paper Equity Chart:**
    *   **Status:** PRESENT. Shows a flat green line at $0.00 over the "last 30 min".
*   **Execution Log:**
    *   **Status:** PRESENT.
    *   **Headers:** TIME, ROUTE, SIZE, NET BPS, P&L, STATUS.
    *   **Row Content:** Contains a single centered message: "waiting for the first paper fill...".
*   **Engine Configuration Form:**
    *   **Status:** PRESENT.
    *   **Fee Input Fields:** PRESENT. Includes a grid of taker fee inputs labeled by venue code (HL, LT, BN, BY, OK, KR, CB, BS, GT). All fields contain numeric values.

---

### 3) CORE PANELS
*   **Orderbook Ladder ("Consolidated — ETH-USD"):** Rendered correctly. No overlapping text or visual glitches. Data is populated for both bid (green) and ask (red) sides with venue tags.
*   **Trade Tape ("Trade Tape ETH-USD"):** Rendered correctly. Scrolling list of trades is visible with time, price, size, and venue columns.
*   **Alerts Panel ("Price Alerts"):** Rendered correctly. Form inputs are visible and aligned. Status text "no alerts yet..." is clear.
*   **Depth History Chart ("Depth History — ETH-USD"):** Rendered correctly. The area chart displays bid/ask depth bands and a mid-price line without clipping or rendering errors.
*   **Venue Books (Bottom Grid):** Rendered correctly. All 9 sub-panels (Hyperliquid, Lighter, Binance, Bybit, OKX, Kraken, Coinbase, Bitstamp, Gate) display their respective stats (BID, ASK, BID SZ, LEVELS).

---

### 4) LAYOUT PROBLEMS
*   **Horizontal Overflow:** None detected. The layout appears contained within the standard viewport width (content edges align with the header).
*   **Misaligned Columns:** None detected. The Orderbook, Trade Tape, Execution Log, and Opportunity tables all show straight vertical alignment for their columns.
*   **Unreadable/Small Text:** Text is legible throughout. Monospaced fonts in the orderbook and tape are crisp against the dark background.
*   **Element Collisions:** No collisions observed. Margins and padding between panels (e.g., between Orderbook and Trade Tape) are consistent.

---

### 5) VERDICT
**Grade: A**

**Defect List:**
1.  **Zero Defects Found.** The interface is visually polished, data-dense, and free of rendering artifacts, layout shifts, or missing elements. The "CONNECTING" status on OKX is a valid state representation, not a UI defect.

## segment-1 (top ~0-1250px: header, venue badges, top panels)

### 1. Venue Badges / Pills (Count: 10)
*   **HYPERLIQUID** LIVE
*   **LIGHTER** LIVE
*   **BINANCE** LIVE
*   **BYBIT** LIVE
*   **OKX** CONNECTING
*   **KRAKEN** LIVE
*   **COINBASE** LIVE
*   **BITSTAMP** LIVE
*   **GATE** LIVE
*   *(Top right)* **WS** OPEN

### 2. Stat Tiles
*   **MID PRICE (USD):** `2474.245` | Subtext: `spread -0.71 · -2.87 bps`
*   **BEST BID (CROSS-VENUE):** `2474.6` | Subtext: `Hyperliquid`
*   **BEST ASK (CROSS-VENUE):** `2473.89` | Subtext: `Lighter`
*   **DEPTH IMBALANCE (0.5% BAND):** `$35.39M / $40.84M` | Subtext: `40% bids` / `54% asks` (with progress bar)

### 3. Table Headers & Data
*   **Consolidated – ETH-USD:**
    *   **Headers:** `PRICE`, `SIZE`, `SUM`, `VENUE`
    *   **Data:** Real data present for both Asks (red) and Bids (green).
    *   **Tabs/Filters:** Consolidated, Hyperliquid, Lighter, Binance, Bybit, Kraken, Coinbase, Bitstamp, Gate.
*   **Trade Tape (ETH-USD):**
    *   **Headers:** `TIME`, `PRICE`, `SIZE`, `VENUE`
    *   **Data:** Real data present with timestamps and trade details.

### 4. Charts
*   **Depth Imbalance Bar Chart:** Renders correctly within the stat tile.
*   **Consolidated Order Book Depth Bars:** Render correctly as horizontal bars behind the price/size text.

### 5. Form Fields
*   **Price Alerts Panel:**
    *   Dropdown: `above` / `below`
    *   Input: `price` (placeholder)
    *   Button: `Set`
    *   Quick set buttons: `-1%`, `+1%`

---

### 6. Visual Defects
1.  **Clipped Text in Trade Tape (Price Column):** In the "Trade Tape" table, several values in the **PRICE** column are significantly wider than the column allows, causing the decimal points to be cut off or overlap with the SIZE column. Examples include `2475.26000000` (row 1), `2474.16 1.212141148` (row 8), and `2474.07 1.21245558` (row 12). The font size or column width is insufficient for the precision of the data being displayed.
2.  **Illogical Spread Calculation (Data Integrity):** The **MID PRICE** is listed as `2474.245`. The **BEST BID** is `2474.6` and the **BEST ASK** is `2473.89`. Mathematically, the Bid is higher than the Ask (an inverted/crossed book), yet the spread is calculated as `-0.71`. Furthermore, a Mid Price of `2474.245` is lower than both the Bid and the Ask, which is logically inconsistent for a standard midpoint calculation between those two values.
3.  **Partially Obscured Row in Trade Tape:** The second to last visible row in the Trade Tape (`09:20:28`) has its price value (`2474.6`) partially obscured by the dark background of the row or a rendering glitch, making it harder to read than the others.
4.  **Inconsistent Decimal Precision:** The "MID PRICE" shows 3 decimals (`2474.245`), while "BEST BID" shows 1 (`2474.6`) and "BEST ASK" shows 2 (`2473.89`). While not strictly a "bug," it lacks visual consistency for a trading terminal where precision is key.

---

### Summary
*   **Total Venue Badges Seen:** 9 (excluding the WS status pill)
*   **Badges saying "live":** 8
*   **Most Notable Defects:**
    1.  **Severe text clipping/overflow** in the Trade Tape price column.
    2.  **Mathematically impossible spread/mid-price** relationship displayed in the top stat tiles (Bid > Ask, but Mid < Both).

## segment-2 (middle ~1150-2400px)

### **Venue Badges / Pills**
*   **Count:** 14
*   **Transcription:**
    *   **BN** (x12): Visible on the right side of the top 12 rows of the upper table.
    *   **HL** (x1): Visible on the row starting with `2747.3`.
    *   **GT** (x1): Visible immediately to the right of the `HL` badge on the same row.

---

### **Stat Tiles**
*   **NET P&L:** `$0.00` (sub-label: `paper`)
*   **FILLS:** `0 / 0 exp` (sub-label: `filled / expired`)
*   **RECORD:** `0W-0L` (sub-label: `win-loss`)
*   **BEST EDGE:** `0 bps` (sub-label: `net bps`)
*   **AVG EDGE:** `0 bps` (sub-label: `per fill`)
*   **OPEN OPPS:** `0` (sub-label: `live routes`)

---

### **Table Headers & Data**
1.  **Live Opportunities**
    *   **Headers:** MARKET, ROUTE (BUY → SELL), NOTIONAL, NET BPS, PROFIT.
    *   **Data Status:** Empty/Placeholder. Contains a single centered message: "scanning 9 venues.. no route above the threshold right now".
2.  **Execution Log**
    *   **Headers:** TIME, ROUTE, SIZE, NET BPS, P&L, STATUS.
    *   **Data Status:** Empty/Placeholder. Contains a single centered message: "waiting for the first paper fill.".

---

### **Charts**
*   **Paper Equity (USD)**
    *   **Status:** Renders correctly as a flat green line at $0.00. Subtitle: "cumulative net P&L · last 30 min".

---

### **Form Fields (Engine Configuration)**
*   **MIN EDGE (BPS):** Value `3`
*   **FIRE EDGE (BPS):** Value `8`
*   **MAX NOTIONAL ($):** Value `10000`
*   **LATENCY (MS):** Value `250`
*   **COOLDOWN (MS):** Value `3000`
*   **TAKER FEES (BPS) Grid:**
    *   HL: `45` | LT: `0` | BN: `10`
    *   BY: `10` | OK: `10` | KR: `26`
    *   CB: `60` | BS: `40` | GT: `20`

---

### **Visual Defects**
1.  **Text Clipping in Upper Table (Top Edge):** The top-most visible row shows a truncated price value `2744.3932`. The top half of the digits is cut off by the segment boundary.
2.  **Inconsistent Data Alignment (Upper Table):** In the column containing values like `59.14`, the value for the `HL/GT` row (`97.26`) and the rows below it appear slightly misaligned or have different padding compared to the top group of `BN` rows.
3.  **Potential Label Collision (Taker Fees):** The label `LT` in the Taker Fees grid is positioned very tightly between the `HL` input and the `BN` input. While legible here, it lacks the breathing room of other labels like `OK` or `KR`.
4.  **Empty State Layout:** The "Live Opportunities" and "Execution Log" panels are largely empty space filled only by a single line of text, which is functionally correct but visually sparse.

---

### **Summary**
*   **Total Venue Badges Seen:** 14
*   **Badges saying "live":** 0 (The word "live" appears in the sub-header text "live USDT/USD normalization" and "live routes", but no badge/pill explicitly contains the text "live").
*   **Most Notable Defects:**
    1.  **Clipped Text at Top Boundary:** The very first row of numerical data is vertically sliced off at the top edge of this image segment.
    2.  **Empty Data Panels:** Both the "Live Opportunities" and "Execution Log" tables are completely devoid of structured row data, showing only placeholder status messages.

## segment-3 (bottom ~2300px-end)

### **Transcription & Analysis of Segment 3 (Bottom ~2300px-end)**

**1. Venue Badges/Pills**
*   **Count:** 9 venue badges visible.
*   **List:**
    *   Hyperliquid (HL)
    *   Lighter (LI)
    *   Binance (BN)
    *   Bybit (BY)
    *   OKX (OK) — Status: **NO FEED**
    *   Kraken (KR)
    *   Coinbase (CB)
    *   Bitstamp (BS)
    *   Gate (GT)

**2. Stat Tiles / Venue Book Data**
Each venue tile contains the following labels and values:
*   **Hyperliquid:** BID: 2474.6, ASK: 2474.7, BID SZ: 31.5647, LEVELS: 40, Latency: 4.7 msg/s
*   **Lighter:** BID: 2473.8, ASK: 2473.89, BID SZ: 4.0426, LEVELS: 1864, Latency: 112.9 msg/s
*   **Binance:** BID: 2475.26, ASK: 2475.27, BID SZ: 15.2389, LEVELS: 40, Latency: 23.3 msg/s
*   **Bybit:** BID: 2474.97, ASK: 2474.98, BID SZ: 5.70577, LEVELS: 100, Latency: 58.5 msg/s
*   **OKX:** BID/ASK/SZ: `waiting...`, LEVELS: 0, Latency: NO FEED badge.
*   **Kraken:** BID: 2473.89, ASK: 2473.9, BID SZ: 45.58828129, LEVELS: 271, Latency: 168.7 msg/s
*   **Coinbase:** BID: 2474.16, ASK: 2474.17, BID SZ: 0.25521148, LEVELS: 2, Latency: 18.1 msg/s
*   **Bitstamp:** BID: 2474.13, ASK: 2474.14, BID SZ: 1.25, LEVELS: 200, Latency: 10.5 msg/s
*   **Gate:** BID: –, ASK: –, BID SZ: –, LEVELS: 0, Latency: 14.3 msg/s

**3. Table Headers**
*   **Venue Books Table:** Headers are `BID`, `ASK`, `BID SZ`, `LEVELS`. Rows contain real data for most venues; OKX and Gate show placeholder or null states.

**4. Charts**
*   **Depth History — ETH-USD:** Renders correctly. Shows bid (green), ask (red), and mid (yellow dashed) lines. Includes Y-axis labels ($41.59M, $27.73M, $13.86M) and X-axis time stamps (09:28 - 09:29).

**5. Form Fields**
*   None visible in this segment.

**6. Visual Defects**
*   **Data Overflow/Misalignment (Critical):** In the **Kraken** venue book tile, the value for **BID SZ** (`45.58828129`) is significantly longer than other values in that column across all other tiles, causing it to visually crowd the adjacent "LEVELS" column header and value.
*   **Missing Data State (Minor):** The **OKX** tile displays a literal string `waiting...` under the BID column instead of a numeric placeholder or dash, which looks inconsistent with the **Gate** tile's use of em-dashes (`–`).
*   **X-Axis Redundancy (Cosmetic):** The X-axis timestamps on the Depth History chart repeat `09:28` and `09:29` twice each without clear differentiation (e.g., seconds), suggesting a labeling bug or insufficient granularity for the selected window.

---

**Summary**
*   **Venue Badges Seen:** 9
*   **"Live" Count:** 0 (None explicitly say "live"; one says "NO FEED").
*   **Most Notable Defects:**
    1.  **Kraken BID SZ Overflow:** The long decimal string `45.58828129` breaks the vertical alignment of the "LEVELS" column within that specific card.
    2.  **Inconsistent Null States:** OKX uses text (`waiting...`) while Gate uses dashes (`–`) for missing data.
    3.  **Chart Time Axis Duplication:** Timestamps on the Depth History chart are duplicated.

