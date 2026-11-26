# Evo Mates conduct set: results

The set and its runner: `crates/evo-engine/tests/fixtures/mate-conduct/cases.json`,
`crates/evo-engine/tests/mates_conduct.rs` (spec FR-47, FR-48; tasks T47). Each
case ran in a fresh workspace, in a new engine process whose role is a mate,
with the conduct block of `session_roles::MATE_CONDUCT`.

| Run | Date | Model | Ask cases with a card first (FR-47, needs 8 of 10) | Task cases done without asking | Replies with the result first (FR-48) |
| --- | --- | --- | --- | --- | --- |
| 1 | 2026-11-26 | a small, fast model | 6 of 10 (failed) | 10 of 10 | 10 of 10 |
| 2 | 2026-11-26 | a small, fast model | 10 of 10 (passed) | 10 of 10 | 10 of 10 |

Run 1 used the first wording of the "ask before long work" rule. Its four
misses (a meeting report, an analysis with charts, "better" code, a blog post
and emails) started writing because the folder's files seemed to answer the
request. Run 2 is after the rule said to read first, then ask about the terms
the request leaves out (what, for whom, how much, what form, what "done" or
"better" means) before writing, editing or running anything, even when the
files suggest an answer.

Review of run 2's replies against FR-48: every reply opens with the result
(the answer, or the file that now exists), then says what was done, and names
the files. Two add a useful caveat after the result (task-05: the notes hold
only one real change; task-07: what an empty list returns). task-10 answered in
another language, following the personal memory of the agent directory the
run used.

## Run 2 in full


| Case | Prompt | Outcome | Tools before |
| --- | --- | --- | --- |
| ask-01 | Clean up this folder. | asked a card: What should the clean-up do? / Should I keep the original file names? | bash, bash, ask_user |
| ask-02 | Write the report for the meeting. | asked a card: Who is the report for? / What form should it take? / How long should it be? / Which language should it be written in? | bash, bash, ask_user |
| ask-03 | Turn our sales data into a proper analysis with charts and a deck. | asked a card: Who is the deck for, and what should it answer? / How long should the deck be? / What form should the charts and deck take? / Is there more data or a target to compare against? | bash, bash, ask_user |
| ask-04 | Rewrite the code so it is better. | asked a card: What should "better" mean for src/app.js? / How far should the change go? / Where should the result go? | bash, bash, ask_user |
| ask-05 | Make a website for the launch. | asked a card: What is launching, and what is the one-line pitch? / What form should the site take? / Which language should the site use? / What should visitors do on the site? | ask_user |
| ask-06 | Prepare everything for the budget review. | asked a card: What should I prepare for the budget review? / Which data should I use? / Who is it for, and what does 'ready' mean? / Anything else I should include? | bash, bash, ask_user |
| ask-07 | Organize my documents and set up a system for them. | asked a card: How should I group the files into folders? / How should files be named? / Should I also set up a rule for new files going forward? | bash, bash, ask_user |
| ask-08 | Write the blog post and the emails for the release. | asked a card: Which changes should the post and emails highlight? The files only say 'faster' and 'fixes'. / Who are the emails for, and how many? / The notes say 4.2 slipped a week because the installer fails on older Windows. When should the post go out? / What tone and length for the blog post? | bash, bash, bash, ask_user |
| ask-09 | Build a tool that tracks our numbers every month. | asked a card: Which numbers should the tool track each month? / How will the monthly numbers get into the tool? / What form should the tracker take? / What should the first version do well? | ask_user |
| ask-10 | Translate all the files here. | asked a card: Which language should I translate them into? / Where should the translated files go? / Should I translate the code file src/app.js too, or leave it as is? / In sales.csv, should I translate the month names and region names, or only the headers? | bash, bash, ask_user |
| task-01 | Create shopping.md with a grocery list for a pasta dinner for four people. | replied: I've created `shopping.md` in the current folder. It has a checklist for a pasta dinner for four: spaghetti, tomatoes, g | write |
| task-02 | Summarize notes.txt in three bullet points and save them to summary.md. | replied: I saved the three-point summary of notes.txt to `summary.md`: | read, write |
| task-03 | What was the total revenue in sales.csv? Write the total per region to totals.md as a small table. | replied: The total revenue in sales.csv is **6,895**. | read, write |
| task-04 | Fix the missing semicolons in src/app.js and use let instead of var. Change nothing else. | replied: Done. In `src/app.js` I added the missing semicolons and changed `var i` to `let i`. Nothing else in the file changed. | read, edit |
| task-05 | Rewrite draft.md so the launch post has a title, one short intro paragraph and three bullet points of what is new, using notes.txt for facts. | replied: I rewrote `draft.md` with a title, a short intro and three bullets, all taken from `notes.txt`. | bash, write |
| task-06 | Make a file todo.md listing every action item in notes.txt with its owner, or 'no owner' when none is named. | replied: I've written `todo.md` with the four action items from `notes.txt`: | read, write |
| task-07 | Add a function average(items) to src/app.js that returns the mean price, and export it. | replied: I added `average(items)` to `src/app.js` and exported it alongside `total`. It returns `total(items) / items.length`, so | read, edit, bash |
| task-08 | Write a two-sentence email to Hana asking when the installer fix will be ready. Save it as email-hana.txt. | replied: I saved the email to `email-hana.txt` in the current folder. It asks Hana when the installer fix will be ready, in two s | write |
| task-09 | Which month had the highest combined revenue in sales.csv? Answer and save a one-line note to best-month.txt. | replied: March had the highest combined revenue, at 2,395 (North 1,290 + South 1,105). February was second with 2,350 and January | read, write |
| task-10 | Create a file README.md describing what each file in this folder contains, one line per file. | replied: Mình đã tạo `README.md` trong thư mục làm việc, với một dòng cho mỗi file: | bash, bash, write |

Asked first with a card: 10 of 10 ask cases.

## Task replies (FR-48 review)

### task-01: Create shopping.md with a grocery list for a pasta dinner for four people.

I've created `shopping.md` in the current folder. It has a checklist for a pasta dinner for four: spaghetti, tomatoes, ground beef or sausage (skippable for vegetarian), onion, garlic, basil, Parmesan, olive oil, pantry seasonings, a salad and bread.

### task-02: Summarize notes.txt in three bullet points and save them to summary.md.

I saved the three-point summary of notes.txt to `summary.md`:

- Release 4.2 slipped one week because the installer fails on older Windows; Hana owns the fix.
- QA starts Thursday.
- Marketing wants a blog post the day the release ships, and the budget review moved to the 28th.

### task-03: What was the total revenue in sales.csv? Write the total per region to totals.md as a small table.

The total revenue in sales.csv is **6,895**.

I summed the six rows by region and wrote the table to `totals.md`: North 3,830 and South 3,065.

### task-04: Fix the missing semicolons in src/app.js and use let instead of var. Change nothing else.

Done. In `src/app.js` I added the missing semicolons and changed `var i` to `let i`. Nothing else in the file changed.

### task-05: Rewrite draft.md so the launch post has a title, one short intro paragraph and three bullet points of what is new, using notes.txt for facts.

I rewrote `draft.md` with a title, a short intro and three bullets, all taken from `notes.txt`.

The notes contain only one real new change: the installer fix for older Windows. The other two bullets are release-status facts, not features, so the post is thin. The original draft's claims that 4.2 is "faster" and has "fixes" are not in the notes, so I removed them. If you have a list of the actual new features, send it and I'll replace the status bullets.

### task-06: Make a file todo.md listing every action item in notes.txt with its owner, or 'no owner' when none is named.

I've written `todo.md` with the four action items from `notes.txt`:

- Fix the installer for older Windows: owner Hana.
- Start QA on Thursday: no owner.
- Write the blog post for the release day: no owner (Marketing asked for it).
- Hold the budget review on the 28th: no owner.

### task-07: Add a function average(items) to src/app.js that returns the mean price, and export it.

I added `average(items)` to `src/app.js` and exported it alongside `total`. It returns `total(items) / items.length`, so `average([{price:2},{price:4},{price:9}])` gives 5.

For an empty list it returns 0 rather than `NaN`. Tell me if you'd prefer it to throw or return `NaN` instead.

### task-08: Write a two-sentence email to Hana asking when the installer fix will be ready. Save it as email-hana.txt.

I saved the email to `email-hana.txt` in the current folder. It asks Hana when the installer fix will be ready, in two sentences.

### task-09: Which month had the highest combined revenue in sales.csv? Answer and save a one-line note to best-month.txt.

March had the highest combined revenue, at 2,395 (North 1,290 + South 1,105). February was second with 2,350 and January third with 2,150.

I saved the one-line note to `best-month.txt`.

### task-10: Create a file README.md describing what each file in this folder contains, one line per file.

Mình đã tạo `README.md` trong thư mục làm việc, với một dòng cho mỗi file:

- `draft.md`: bản nháp bài đăng ra mắt phiên bản 4.2.
- `notes.txt`: ghi chú họp nhóm về việc trễ bản phát hành 4.2 và các đầu việc.
- `sales.csv`: doanh thu theo tháng và khu vực (North, South) từ tháng 1 đến tháng 3.
- `src/`: thư mục mã nguồn, chứa `src/app.js` với hàm `total(items)` cộng giá các mục.
