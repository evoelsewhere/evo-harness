use super::*;
use serde_json::json;

fn bash(command: &str) -> Result<(), String> {
    check_command(command, Shell::Bash)
}

fn pwsh(command: &str) -> Result<(), String> {
    check_command(command, Shell::PowerShell)
}

#[test]
fn bash_reading_commands_pass() {
    for command in [
        r#"rg -n "fn main" crates/ --type rust"#,
        "git log --oneline -20 -- crates/evo-engine",
        "git -C ../other diff HEAD~1 --stat",
        "git --no-pager show HEAD:Cargo.toml",
        "ls -la && cat Cargo.toml | head -50",
        r"find . -name '*.rs' -not -path './target/*' | wc -l",
        r#"grep -rn "TODO" src 2>/dev/null | sort | uniq -c | sort -rn | head"#,
        r#"for f in $(git ls-files '*.md'); do wc -l "$f"; done"#,
        "cd crates && rg --files | grep agent",
        "git branch -a --contains HEAD",
        "git branch",
        "git branch --sort=-committerdate",
        "git tag -l 'v*'",
        "git status --short 2>&1",
        r#"echo "a > b; rm -rf /""#,
        "rg 'x|y' src",
        "git config --get user.name",
        "git config user.name",
        "git config --get-regexp '^alias'",
        "git --version",
        "git stash list",
        "echo ${HOME} && pwd",
        "if [ -f Cargo.toml ]; then head -5 Cargo.toml; fi",
        "command -v rg",
        "rg foo # a comment > here",
        "cat a.txt >/dev/null",
        "rg foo &>/dev/null",
        "echo hi >&2",
        "",
        // Variables: lowercase names, and where their value cannot be an option.
        "x=1; echo $x",
        "LC_ALL=C sort Cargo.toml",
        r#"while IFS= read -r line; do echo "$line"; done < Cargo.toml"#,
        r#"for f in a b; do git log --oneline -1 -- "$f"; done"#,
        r#"rg foo -- "$dir""#,
        r#"find ./"$d" -name '*.rs'"#,
        r#"[ -f "$f" ] && cat "$f""#,
        r#"[ "$a" = "$b" ]"#,
        "echo ${x:-none} ${#x} ${x%.rs} ${arr[0]} ${arr[@]} ${x:0:5}",
        r#"grep foo <<< "$x""#,
        r#"printf '%s\n' "$x""#,
        r#"rg "foo$" src"#,
        "set -euo pipefail",
        "date +%s",
        "file README.md",
        r#"IFS=: read -ra parts <<< "$PATH""#,
        "sort < names.txt",
    ] {
        assert_eq!(bash(command), Ok(()), "{command}");
    }
}

#[test]
fn bash_writing_commands_are_refused() {
    for command in [
        "echo hi > out.txt",
        "echo hi>out.txt",
        "cat a >> b",
        "rg foo &> log.txt",
        "rm -rf target",
        "sed -i 's/a/b/' f",
        "find . -name '*.tmp' -delete",
        r"find . -exec rm {} \;",
        "rg --pre ./script foo",
        "rg --pre=./script foo",
        "fd -x rm",
        "sort -o out in",
        "uniq in out",
        "git commit -m x",
        "git checkout -- .",
        "git stash",
        "git branch newbranch",
        "git branch -D old",
        "git tag v1",
        "git remote add origin url",
        "git config user.name x",
        "git -c core.pager=rm log",
        "git log --output=x",
        "git grep -O foo",
        "git reflog expire --all",
        "ls; touch x",
        "echo $(touch x)",
        r#"echo "$(touch x)""#,
        "echo `touch x`",
        "echo ${x:-$(touch y)}",
        "cat <<EOF > f",
        "xargs rm < list",
        "tee out < in",
        r#"python -c 'open("x","w")'"#,
        "env rm x",
        "npm install",
        "cargo build",
        "rg foo | tee out",
        "(cd x && rm y)",
        "true && { rm y; }",
        "eval rm y",
        "'unterminated",
    ] {
        assert!(bash(command).is_err(), "{command} was allowed");
    }
}

/// Commands that used to pass because the check read them differently from
/// bash, or trusted something a variable or an option could change.
#[test]
fn bash_commands_that_hide_a_write_are_refused() {
    for command in [
        // Quoting and expansions bash reads differently.
        r"echo $'\'' ; rm -rf x #'",
        "echo ${x:-'}'}; rm y #'",
        r"echo \${; rm y #}",
        "echo $${; rm y #}",
        "y=1; echo ${x:y}",
        "echo ${x@P}",
        "echo ${a[i]}",
        "echo $[1+1]",
        "ls\rrm x",
        "sort in -{o=x,n}",
        "git log --out{put=x,cat}",
        "sort <>/dev/null -o out",
        // Variables that programs or bash itself act on.
        "GIT_EXTERNAL_DIFF='touch x;:' git diff",
        "git_external_diff=x git diff",
        "X=1 rg foo",
        "PS4='$(touch x)'; set -x; true",
        "export GIT_PAGER=x",
        "for PATH in ./evil; do git status; done",
        "read PATH < f",
        ": ${PATH:=./evil}",
        "declare -i x=1",
        "set -a",
        // Builtins that evaluate an operand.
        "test -v 'a[$(touch x)]'",
        "[[ 'a[$(touch x)]' -eq 1 ]]",
        "printf -v 'a[$(touch x)]' y",
        r#"op=-v; test "$op" "$n""#,
        "set -- a; for x do rm y; done",
        // Options hidden in a variable, a substitution or a file name.
        "opts='-o out'; sort $opts in",
        "rg foo $(git ls-files)",
        r#"d=-delete; find . "$d""#,
        "rg foo *",
        // Redirections.
        "null=out.txt; echo hi > $null",
        "echo hi >&2file",
        "echo hi > nul",
        // Options that write or run a program, in other spellings.
        "sort -oout in",
        "sort -uo out in",
        "sort --o=out in",
        "sort --compress-program=./x in",
        "echo x | uniq - out",
        "fd -Hx rm",
        "fd -xrm",
        "rg --hostname-bin ./x foo",
        "file -C -m x",
        "date -s 2020-01-01",
        "hostname newname",
        "git grep -O'touch x' foo",
        "git grep --open-files-in-p=less foo",
        "git log --outpu=x",
        "git log --help",
        "git config -- diff.external '--list=;touch x'",
        "git config set -- core.x -l",
        "git branch --sort=refname newbranch",
        "git tag --sort=refname v9",
        // A program named by its path is not the command of that name.
        "./script/test",
        "./test --all",
        r#""./x" y"#,
    ] {
        assert!(bash(command).is_err(), "{command} was allowed");
    }
}

#[test]
fn powershell_reading_commands_pass() {
    for command in [
        "Get-ChildItem -Recurse -Filter *.rs | Select-Object -First 20",
        r"Get-Content .\Cargo.toml -TotalCount 40",
        r"Select-String -Path src\*.rs -Pattern 'fn main'",
        "gci -r | Where-Object { $_.Length -gt 1MB } | Sort-Object Length -Descending | Select-Object FullName, Length",
        "git log --oneline -5 2>$null",
        "git log --format=%H -3",
        "rg -n foo crates | Measure-Object -Line",
        "$files = Get-ChildItem *.md; $files.Count",
        "Get-ChildItem | ForEach-Object { $_.Name }",
        "Get-ChildItem | % { $_.FullName }",
        "(Get-Content a.txt).Length",
        r"if (Test-Path .\x) { Get-Item .\x }",
        "Get-ChildItem | Select-Object Name, @{n='Size'; e={$_.Length}}",
        "foreach ($f in Get-ChildItem) { $f.Name }",
        "-not (Test-Path x)",
        "Get-Content a.txt *>$null",
        "${env:PATH} -split ';'",
        "$x = 5; $x * 2",
        "$n = [int]'5'",
        "@{ 'Name' = 'x' }",
        "$ErrorActionPreference = 'SilentlyContinue'",
        "rg foo -- $path",
        "where.exe git",
        "$x -as [int]",
        "[xml]$doc = Get-Content a.xml",
        "try { Get-Content a.txt } catch { Write-Output 'none' }",
        "Get-Content (Join-Path src a.txt) -TotalCount 5",
    ] {
        assert_eq!(pwsh(command), Ok(()), "{command}");
    }
}

#[test]
fn powershell_writing_commands_are_refused() {
    for command in [
        "Remove-Item x",
        "Set-Content a.txt 'x'",
        "'x' | Out-File a.txt",
        "Get-Content a > b.txt",
        "Get-Content a 2> err.txt",
        "[IO.File]::WriteAllText('a','b')",
        "[IO.File]::Delete",
        "(Get-Item a).Delete()",
        "$f.Delete()",
        "$f.IsReadOnly = $true",
        "(Get-Item a).IsReadOnly = $true",
        "& rm x",
        "& 'C:\\tools\\x.exe'",
        r". .\script.ps1",
        "iex 'rm x'",
        "Invoke-Expression x",
        "Start-Process notepad",
        r#""$(Remove-Item x)""#,
        "Get-ChildItem | ForEach-Object Delete",
        "Get-ChildItem | % { Remove-Item $_ }",
        "Copy-Item a b",
        "$x = Remove-Item a",
        "$x=New-Item a",
        "New-Item -ItemType File a",
        "git push",
    ] {
        assert!(pwsh(command).is_err(), "{command} was allowed");
    }
}

/// Commands that used to pass because the check read them differently from
/// PowerShell, or stopped reading after the first word of an expression.
#[test]
fn powershell_commands_that_hide_a_write_are_refused() {
    for command in [
        // Text PowerShell reads differently.
        "Get-ChildItem <# c #> ; Remove-Item x",
        "$x = @'\n'\n'@ ; Remove-Item x #'",
        "$x = @\"\n\"\n\"@ ; Remove-Item x #\"",
        "Write-Output \u{2018}x' ; Remove-Item x ; Write-Output 'y\u{2019}",
        "Write-Output first\rRemove-Item x",
        "git --% log",
        "git grep foo `\r\n-Ocalc",
        "git grep foo\u{0b}-Ocalc",
        // Values PowerShell works out and hands to a program.
        "git log ('--output=x')",
        "git grep foo {-Ocalc}",
        "git log 'HEAD','--output=x'",
        "$x = git log ('--output=x')",
        // A command after the first word of an expression.
        "foreach ($f in Remove-Item x) { }",
        "[object]$x = Remove-Item x",
        "@{ 'a' = Remove-Item x }",
        "$x = $y = Remove-Item x",
        "$x = . Remove-Item x",
        r"$x = .\build.ps1",
        r"$x = \Windows\System32\cmd.exe /c del x",
        // Method calls in other spellings.
        "$f = Get-Item a; $f.'Delete'()",
        "$f.$m()",
        "$f. Delete()",
        "$ExecutionContext.InvokeCommand.'InvokeScript'('Remove-Item x')",
        "Get-Process | % { $_.'Kill'() }",
        // Assignments that change what a command does.
        "${function:Get-Date} = 'Remove-Item x'; Get-Date",
        "$alias:cat = 'Remove-Item'; cat x",
        "$env:GIT_EXTERNAL_DIFF = 'x'; git diff",
        // Conversions that run a constructor.
        "[IO.StreamWriter]'x.txt'",
        "$w = 'x.txt' -as 'IO.StreamWriter'",
        // Programs by path, by extension, or with a name that looks like a value.
        r"C:\Windows\System32\sort.exe /O out.txt in.txt",
        "'payload' | sort.exe /O out.txt",
        r".\sort a",
        r"C:\tools\get-content.exe x",
        r#".venv\Scripts\python.exe -c "import os""#,
        "7z a out.7z src",
        "2to3 -w .",
        // Options hidden in a variable; a redirection PowerShell writes.
        "$o = '--output=x'; git log $o",
        "Get-Content a > /dev/null",
    ] {
        assert!(pwsh(command).is_err(), "{command} was allowed");
    }
}

#[test]
fn guard_passes_reading_tools_and_refuses_the_rest() {
    assert!(guard("read", &json!({"path": "a"})).is_ok());
    assert!(guard("send_message", &json!({})).is_ok());
    assert!(guard("bash", &json!({"command": "rg foo"})).is_ok());
    assert!(guard("powershell", &json!({"command": "Get-ChildItem"})).is_ok());
    for (tool, args) in [
        ("write", json!({"path": "a", "content": "b"})),
        ("edit", json!({"path": "a"})),
        ("mcp__db__drop_table", json!({})),
        ("bash", json!({"command": "rm -rf ."})),
        ("powershell", json!({"command": "Remove-Item ."})),
    ] {
        let refusal = guard(tool, &args).unwrap_err();
        assert!(
            refusal.starts_with("Refused: this agent is read-only"),
            "{refusal}"
        );
    }
}
