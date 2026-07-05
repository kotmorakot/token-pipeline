# หลักการทำงานของ Filter — ทำไมข้อมูลไม่พัง ไม่หาย

> อัพเดต v1.0.0 — tp เป็น standalone เต็มรูปแบบ ไม่ต้องพึ่ง rtk อีกต่อไป

## สำหรับใคร
เอกสารนี้สำหรับ Developer ที่อยากเข้าใจว่า "ระบบ filter" ทำงานยังไง ทำไมตัดข้อมูลออกแล้วเนื้อหาไม่เสียหาย พร้อมตัวอย่างจริงจากทั้ง 3 แนวทาง:
- **RTK (แนวคิดดั้งเดิม)** — filter output ของ command ก่อนส่งให้ AI
- **Caveman** — สอน AI ให้ตอบสั้นลง ตัดคำเยิ่นเย้อ
- **Token Pipeline (tp) v1.0.0** — รวมทั้ง 2 แนวทาง + cache + rewrite engine + async proxy

## คำสั่งใหม่ใน v1.0.0
- `tp read <file>` — อ่านไฟล์แบบ smart (ดึง signature จากไฟล์ใหญ่)
- `tp rewrite <cmd>` — ดูว่า tp จะ rewrite compound command อย่างไร
- `tp init auto` — auto-detect agent ที่ติดตั้งแล้ว configure ให้ทั้งหมด
- `tp config init` — สร้าง config file ที่ `~/.config/tp/config.toml`

---

## 1. หลักการพื้นฐาน: Signal vs Noise

ทุกระบบ filter ทำงานบนหลักการเดียวกัน:

```
ข้อมูลทั้งหมด = Signal (สิ่งที่มีความหมาย) + Noise (สิ่งที่ไม่จำเป็น)
```

**Signal** = ข้อมูลที่ AI ต้องใช้ในการทำงาน เช่น:
- ชื่อไฟล์ที่เปลี่ยน
- Error message
- โค้ดที่ต้องแก้ไข
- คำสั่งที่ต้องรัน

**Noise** = ข้อมูลที่ AI ไม่ต้องใช้ เช่น:
- ข้อความ "On branch main" (รู้อยู่แล้ว)
- Progress bar: "Downloading 45%... 46%... 47%..."
- คำแนะนำ: `(use "git add" to track)`
- ข้อความซ้ำ 100 บรรทัด
- Decoration: `═══════════════════`

### กฎเหล็ก: ตัดแต่ Noise ไม่ตัด Signal

```
✅ ตัดได้ (Noise):
   "On branch main"                          → (ไม่จำเป็น ดูจาก prompt ได้)
   "Your branch is up to date with 'origin'" → (ไม่จำเป็น)
   "(use 'git add' to include...)"           → (AI รู้อยู่แล้ว)

❌ ห้ามตัด (Signal):
   "modified: src/main.rs"                   → (ต้องรู้ว่าไฟล์ไหนเปลี่ยน)
   "error[E0308]: mismatched types"          → (ต้องเห็น error ทั้งหมด)
   "+    let x = 42;"                        → (ต้องเห็นโค้ดที่เปลี่ยน)
```

---

## 2. RTK / Smart Proxy — Filter Command Output

### 2.1 ทำงานยังไง

```
User/Agent → tp run git status → รัน git status จริง → filter output → ส่ง output ที่กรองแล้ว
                                      ↓                    ↓
                                 raw output           filtered output
                                 (20 บรรทัด)           (4 บรรทัด)
```

tp (หรือ RTK) ทำตัวเป็น **คนกลาง** (middleware):
1. รับคำสั่งจาก user
2. รัน command จริง 100%
3. เอา output มา filter ตาม rules ที่กำหนดไว้ต่อ command
4. ส่ง output ที่กรองแล้วกลับไป

### 2.2 ตัวอย่างจริง: git status

**ก่อน filter (raw output — 15 บรรทัด):**
```
On branch main
Your branch is up to date with 'origin/main'.

Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        new file:   src/pipeline.rs
        modified:   Cargo.toml

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified:   src/main.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
        README.md
```

**หลัง filter (5 บรรทัด):**
```
[main]
staged:
  + src/pipeline.rs
  + Cargo.toml
modified:
  M src/main.rs
untracked:
  ? README.md
```

**อะไรถูกตัดออก? (Noise)**
| สิ่งที่ตัด | ทำไม |
|-----------|------|
| "On branch main" | → เหลือแค่ `[main]` สั้นกว่า |
| "Your branch is up to date..." | → AI ไม่ต้องการข้อมูลนี้ |
| "(use "git restore...")" | → AI รู้คำสั่ง git อยู่แล้ว |
| "(use "git add...")" | → คำแนะนำที่ไม่จำเป็น |
| บรรทัดว่าง | → ลดพื้นที่ |

**อะไรที่ยังอยู่ครบ? (Signal)**
| สิ่งที่เหลือ | ทำไม |
|-------------|------|
| ชื่อ branch (main) | → ต้องรู้ว่าอยู่ branch ไหน |
| ชื่อไฟล์ทุกไฟล์ | → ต้องรู้ว่าไฟล์ไหนเปลี่ยน |
| สถานะของแต่ละไฟล์ (+, M, ?) | → ต้องรู้ว่า staged/modified/untracked |

### 2.3 ตัวอย่างจริง: git diff

**ก่อน filter:**
```
diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,7 +10,8 @@ fn main() {
     let args: Vec<String> = env::args().collect();
-    println!("Hello, world!");
+    println!("Hello, token pipeline!");
+    println!("Version: 0.1.0");
```

**หลัง filter:**
```
--- src/main.rs (+2 -1) ---
  @@ -10,7 +10,8 @@
  -    println!("Hello, world!");
  +    println!("Hello, token pipeline!");
  +    println!("Version: 0.1.0");
```

**ตัดอะไร?**
- `diff --git a/src/main.rs b/src/main.rs` → ชื่อไฟล์อยู่แล้ว
- `index abc1234..def5678 100644` → hash ไม่จำเป็น
- `--- a/src/main.rs` / `+++ b/src/main.rs` → ซ้ำกับหัว
- Context lines ที่ไม่เปลี่ยน → ไม่จำเป็น (เห็น +/- ก็พอ)

**ไม่ตัดอะไร?**
- ทุกบรรทัดที่ขึ้นต้นด้วย `+` (เพิ่ม) หรือ `-` (ลบ)
- Hunk header `@@ ... @@` (บอกตำแหน่ง)
- ชื่อไฟล์

### 2.4 ตัวอย่าง: cargo test

**ก่อน filter (ผ่านทุก test — 30 บรรทัด):**
```
   Compiling my-project v0.1.0
    Finished test [unoptimized + debuginfo] target(s)
     Running unittests src/lib.rs (target/debug/deps/my_project-abc123)

running 12 tests
test test_add ... ok
test test_subtract ... ok
test test_multiply ... ok
test test_divide ... ok
test test_parse_json ... ok
test test_validate ... ok
test test_cache_hit ... ok
test test_cache_miss ... ok
test test_filter_git ... ok
test test_filter_ls ... ok
test test_compress_lite ... ok
test test_compress_full ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**หลัง filter (1 บรรทัด!):**
```
ok test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**ทำไมตัดได้ขนาดนี้?**
เพราะเมื่อ test ผ่านทุกตัว สิ่งเดียวที่ AI ต้องรู้คือ **"ผ่านหมด"**
ไม่ต้องเห็นรายละเอียดว่า test ไหนผ่าน

**แต่ถ้ามี test FAIL:**
```
FAILED: 1 failures

--- failure 1 ---
test test_divide ... FAILED
thread 'test_divide' panicked at 'attempt to divide by zero'
  src/math.rs:15

test result: ok. 11 passed; 1 failed; 0 ignored
```

**สังเกต:** เมื่อ fail → แสดง error ทั้งหมด ไม่ตัดอะไรเลย!
เพราะ error = Signal ที่สำคัญที่สุด

### 2.5 หลักการ: filter ตาม command

| Command | กลยุทธ์ filter | เหตุผล |
|---------|--------------|--------|
| `git status` | ตัด hints, เหลือเฉพาะไฟล์+สถานะ | AI รู้ git อยู่แล้ว |
| `git diff` | ตัด context lines, เหลือ +/- | ดู diff ขอแค่ change |
| `git log` | one-line format | ข้อมูล date/author ย่อได้ |
| `cargo test` | ถ้าผ่าน=สรุป, fail=แสดงหมด | ถ้าผ่านไม่ต้องดูรายละเอียด |
| `cargo build` | เหลือแค่ error/warning | ไม่ต้องเห็น Compiling X |
| `ls` | compact format | ตัด permissions, size, date |
| `grep` | group by file | ลด repetition |
| `find` | group by directory | ลด path repetition |
| `env` | mask secrets | ความปลอดภัย |
| `curl` | truncate ถ้ายาว | response body อาจยาวมาก |

---

## 3. Caveman — สอน AI ให้ตอบสั้น

### 3.1 แนวคิด

Caveman ไม่ได้ filter output ของ command
แต่ **สอน AI ให้ตอบสั้นลง** โดยตัดคำที่ไม่จำเป็นออก

```
ปกติ AI ตอบ:
"Sure! I'd be happy to help you with that. The issue you're experiencing
is likely caused by your authentication middleware not properly validating
the token expiry. Let me take a look and suggest a fix."

Caveman mode:
"Bug in auth middleware. Token expiry check use < not <=. Fix:"
```

### 3.2 อะไรถูกตัด (Noise ในภาษา)

| ประเภท | ตัวอย่าง | ทำไมตัดได้ |
|--------|---------|-----------|
| **คำเยิ่นเย้อ (Filler)** | just, really, basically, actually, simply | ไม่เพิ่มความหมาย |
| **คำสุภาพ (Pleasantries)** | Sure!, Of course!, Happy to help! | ไม่เพิ่มข้อมูล |
| **คำเลี่ยง (Hedging)** | I think, It seems like, probably | AI ควรตอบตรงๆ |
| **Articles** | a, an, the | ภาษาเข้าใจได้โดยไม่มี |
| **คำซ้ำ (Narration)** | Let me explain, Here's what's happening | บอกก่อนทำ ไม่จำเป็น |

### 3.3 อะไรที่ Caveman ไม่แตะ (Signal)

| ประเภท | ตัวอย่าง | ทำไมไม่ตัด |
|--------|---------|-----------|
| **Code blocks** | `fn main() {}` | ต้องถูก 100% |
| **Error messages** | `error[E0308]` | ต้องเห็นทั้งหมด |
| **File paths** | `src/main.rs` | ต้องถูกต้อง |
| **URLs** | `https://api.example.com` | เปลี่ยนแม้แต่ตัวเดียวก็พัง |
| **CLI commands** | `cargo build --release` | ต้อง exact |
| **Technical terms** | `useMemo`, `async/await` | คำเฉพาะทาง |
| **Numbers** | `port 8080`, `version 3.2` | ตัวเลขต้องถูก |

### 3.4 ตัวอย่าง 3 ระดับ

**คำถาม:** "ทำไม React component re-render?"

**Lite mode** (ตัดแค่คำเยิ่นเย้อ):
> Your component re-renders because you create a new object reference each render. Wrap it in `useMemo`.

**Full mode** (ตัด articles + ใช้คำสั้น):
> New object ref each render. Inline object prop = new ref = re-render. Wrap in `useMemo`.

**Ultra mode** (สั้นสุดๆ):
> Inline obj prop, new ref, re-render. `useMemo`.

**สังเกต:** ทั้ง 3 ระดับ `useMemo` ไม่เปลี่ยนเลย!

### 3.5 วิธีทำงาน

Caveman ใช้ **System Prompt** สอน AI:
```
Respond terse. Drop articles (a/an/the), filler (just/really/basically),
pleasantries (sure/certainly/of course). Fragments OK. Short synonyms.
Code blocks, commands, errors, paths: byte-for-byte exact.
```

เพิ่มเติม: Token Pipeline ยัง **post-process** response อีกชั้นหนึ่ง
เช่น ถ้า AI ลืมตัดคำว่า "basically" ระบบก็ตัดให้อัตโนมัติ

---

## 4. Token Pipeline — รวมทุกอย่าง

### 4.1 Full Pipeline

```
┌──────────────────────────────────────────────────────────────────────┐
│                     Token Pipeline (tp)                               │
│                                                                      │
│  Stage 1: INPUT FILTER (RTK-style)                                   │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │ Command Output → ตัด Noise → เหลือ Signal                      │  │
│  │ git status (15 lines) → compact (5 lines)                      │  │
│  │ cargo test pass (30 lines) → "ok 12 passed" (1 line)          │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                              ↓                                       │
│  Stage 2: OPTIMIZER (KatGPT-RS-style)                                │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │ • BLAKE3 Cache: ถามซ้ำ → ตอบจาก cache ทันที (ไม่เรียก LLM)     │  │
│  │ • Prompt Compression: ตัดคำเยิ่นเย้อจาก prompt                  │  │
│  │ • Constraint Validation: ตรวจ JSON → แก้เฉพาะจุด                │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                              ↓                                       │
│  Stage 3: OUTPUT COMPRESS (Caveman-style)                            │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │ LLM Response → ตัดคำเยิ่นเย้อ → เหลือแค่ essence               │  │
│  │ "Sure! I'd be happy..." (50 tokens) → "Fix:" (3 tokens)       │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                              ↓                                       │
│  Result: ประหยัด 40-70% tokens                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 4.2 ใช้ 2 โหมด

**โหมด CLI (tp run):**
```bash
tp run git status      # แทน git status
tp run cargo test      # แทน cargo test
tp run ls -la          # แทน ls -la
```
→ ใช้ Stage 1 เท่านั้น (filter command output)

**โหมด Proxy (tp proxy):**
```bash
tp proxy --port 8080 --upstream http://your-llm:8000
# แล้วชี้ IDE ไปที่ http://localhost:8080
```
→ ใช้ทั้ง 3 Stages (full pipeline)

---

## 5. ทำไมข้อมูลไม่พัง?

### 5.1 กฎ "Never Touch" — สิ่งที่ห้ามแตะ

ทุก filter ใน tp มีกฎตายตัว: **ห้ามแก้ไข** สิ่งต่อไปนี้

| ประเภท | ตัวอย่าง | เหตุผล |
|--------|---------|--------|
| Exit code | `exit 0`, `exit 1` | บอก success/failure |
| Error messages | `error[E0308]: mismatched types` | ต้องเห็นเต็ม |
| Stack traces | `at src/main.rs:42` | ต้องรู้ตำแหน่ง |
| Code content | `fn main() { }` | เปลี่ยน 1 ตัวอักษรก็พัง |
| File paths | `src/pipeline.rs` | เปลี่ยนชื่อไฟล์ = ผิดที่ |
| URLs | `http://localhost:8080` | เปลี่ยน port = พัง |
| Command syntax | `cargo build --release` | ต้อง exact |
| Numbers/versions | `v0.1.0`, `port 3000` | ตัวเลขต้องถูก |

### 5.2 กลยุทธ์ "ถ้าไม่แน่ใจ ไม่ตัด"

Filter ทุกตัวถูกออกแบบให้ **conservative**:
- ถ้าไม่รู้จัก command → ส่ง output ดิบ (ไม่ filter เลย)
- ถ้า output สั้นอยู่แล้ว (<50 บรรทัด) → ส่งดิบ
- ถ้า exit code ≠ 0 → แสดง error ทั้งหมด
- ถ้ามี code block → ส่งผ่านโดยไม่แก้ไข

```rust
// ตัวอย่างจากโค้ด: ถ้าสั้นอยู่แล้ว ไม่ตัด
fn generic_compact(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}{}", stdout, stderr);
    let lines: Vec<&str> = combined.lines().collect();
    if lines.len() <= 50 {
        return combined;  // สั้นอยู่แล้ว ส่งกลับเลย
    }
    // ... ตัดเฉพาะถ้ายาว
}
```

### 5.3 ตัวอย่าง: Error ไม่ถูกตัด

**Input (cargo test fail):**
```
test test_divide ... FAILED
thread 'test_divide' panicked at 'attempt to divide by zero', src/math.rs:15:5
stack backtrace:
   0: rust_begin_unwind
   1: core::panicking::panic_fmt
   2: my_project::math::divide
             at ./src/math.rs:15:5
   3: my_project::tests::test_divide
             at ./src/math.rs:28:9

test result: ok. 11 passed; 1 failed; 0 ignored
```

**Output (filter แสดง error ทั้งหมด):**
```
FAILED: 1 failures

--- failure 1 ---
test test_divide ... FAILED
thread 'test_divide' panicked at 'attempt to divide by zero', src/math.rs:15:5
stack backtrace:
   0: rust_begin_unwind
   1: core::panicking::panic_fmt
   2: my_project::math::divide
             at ./src/math.rs:15:5

test result: ok. 11 passed; 1 failed; 0 ignored
```

**สังเกต:** error + stack trace อยู่ครบทุกบรรทัด!

---

## 6. เปรียบเทียบ 3 แนวทาง

| | RTK/SP/tp run | Caveman | tp proxy |
|---|---|---|---|
| **ทำอะไร** | Filter command output | สอน AI ตอบสั้น | ทั้ง filter + compress |
| **ลดอะไร** | Input tokens (context) | Output tokens (response) | ทั้ง input + output |
| **ลดได้เท่าไหร่** | 30-80% ต่อ command | ~65% output tokens | 40-70% overall |
| **ใช้กับ** | CLI, AI agents | AI agents ทุกตัว | IDE, CLI, agents |
| **ข้อดี** | ง่าย, ไม่เสี่ยง | ประหยัดมาก | ครบจบ |
| **ข้อจำกัด** | ไม่ลด response | เพิ่ม input tokens ~1K | ต้องรัน proxy |

### 6.1 เมื่อไหร่ใช้แบบไหน

**ใช้ RTK/tp run เมื่อ:**
- ใช้ AI agent ที่รัน CLI commands (Hermes, Claude Code, Codex)
- ต้องการลด context window usage
- ต้องการ setup ง่ายๆ

**ใช้ Caveman เมื่อ:**
- ใช้ AI agent ที่ตอบยาวเกินไป
- ต้องการลด output token cost (pay-as-you-go)
- ต้องการ response ที่อ่านง่าย

**ใช้ tp proxy เมื่อ:**
- ใช้ pay-as-you-go LLM (OpenAI, Claude API)
- ต้องการลด cost ทุกทาง
- ต้องการ cache responses

---

## 7. ข้อมูลเชิงลึก: ทำไม Filter ปลอดภัย

### 7.1 Whitelist vs Blacklist

ระบบ filter ใช้แนวทาง **"ทำเท่าที่รู้"**:

```
✅ Whitelist approach (ที่เราใช้):
   รู้จัก "git status" → ใช้ git_status filter
   รู้จัก "cargo test" → ใช้ test_compact filter
   ไม่รู้จัก "some-custom-cmd" → ส่ง output ดิบ (ไม่ filter!)

❌ Blacklist approach (ที่เราไม่ใช้):
   ลบทุกบรรทัดที่ดู "ไม่สำคัญ" → เสี่ยง!
```

### 7.2 Exit Code Preservation

```rust
// tp run จะ exit ด้วย code เดียวกับ command จริง
std::process::exit(out.status.code().unwrap_or(0));
```

ถ้า `cargo test` fail (exit code 1):
- tp run ก็ exit ด้วย code 1
- AI agent เห็น exit code 1 → รู้ว่า fail
- Error message แสดงครบ

### 7.3 Code Block Protection

```rust
// ใน output_compress: ตรวจว่าอยู่ใน code block ไหม
for line in text.lines() {
    if trimmed.starts_with("```") {
        in_code_block = !in_code_block;  // toggle
    }
    if in_code_block {
        // อยู่ใน code block → ส่งผ่านเลย ไม่แก้ไข!
        segments.push(Segment::Code(...));
    } else {
        // อยู่นอก code block → compress ได้
        segments.push(Segment::Prose(...));
    }
}
```

---

## 8. สรุป

### สำหรับ Junior Developer ที่กังวลว่า "ตัดแล้วจะหายไหม?"

1. **ไม่หาย** — filter ตัดแค่ "เครื่องประดับ" (decoration/noise) ไม่ตัด "เนื้อหา" (content/signal)
2. **ปลอดภัย** — ถ้าไม่รู้จัก command ก็ไม่ filter (whitelist approach)
3. **Error แสดงครบ** — เมื่อ exit code ≠ 0 แสดง error ทั้งหมด
4. **Code ไม่ถูกแก้** — code blocks ถูก protect ไว้เสมอ
5. **ย้อนกลับได้** — ถ้า filter ผิดพลาด แค่รัน command โดยไม่ผ่าน tp

### วิธีทดสอบด้วยตัวเอง

```bash
# เปรียบเทียบ output ก่อน-หลัง filter
git status > /tmp/raw.txt
tp run git status > /tmp/filtered.txt
diff /tmp/raw.txt /tmp/filtered.txt

# ดูว่า tp ประหยัดเท่าไหร่
tp stats
```

### เปรียบเทียบง่ายๆ

```
Filter = กรองน้ำ
น้ำ (ข้อมูล) ไหลผ่านตะแกรง (filter)
สิ่งสกปรก (noise) ถูกกรองออก
น้ำสะอาด (signal) ผ่านไปได้หมด

ถ้าไม่มีตะแกรงที่เหมาะ → ปล่อยน้ำผ่านไปเลย (ไม่กรอง)
ดีกว่ากรองผิดแล้วน้ำสะอาดหาย
```

---

*เอกสารนี้เป็นส่วนหนึ่งของ Token Pipeline project*
*สร้างเพื่อ Junior Developer ที่ต้องการเข้าใจหลักการ filter อย่างลึกซึ้ง*
