//! Running a project's tests headless (see `docs/specs/testing.md`).
//!
//! `gdt` defines no test framework. It finds the one the project uses (GUT or
//! gdUnit4, or a script of its own), builds the command line Godot needs, and
//! reads the JUnit report that comes back, because the runners' exit codes alone
//! are not trustworthy: GUT exits with 0 when it ran nothing at all, or when its
//! classes were never imported.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::manifest::TestConfig;

/// Errors from setting up a test run.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error(
        "no test runner found: install GUT or gdUnit4 (`gdt addon add --dev github:bitwes/Gut`), \
         or set [test] runner and script in gdt.toml"
    )]
    NoRunner,
    #[error(
        "both GUT and gdUnit4 are installed; say which one to use with [test] runner in gdt.toml"
    )]
    BothRunners,
    #[error("`{0}` is not a test runner; use \"gut\", \"gdunit4\" or \"script\"")]
    UnknownRunner(String),
    #[error("runner = \"{runner}\" but addons/{folder} is not installed; run `gdt sync`")]
    RunnerMissing {
        runner: &'static str,
        folder: &'static str,
    },
    #[error(
        "runner = \"script\" needs `script` in [test] (a GDScript that exits non-zero on failure)"
    )]
    NoScript,
    #[error("the test script {0} does not exist")]
    ScriptMissing(String),
    #[error("{0} is outside the project")]
    OutsideProject(String),
    #[error("there is no folder or file {0} to take tests from; set [test] paths in gdt.toml")]
    NoTests(String),
}

/// A test framework gdt knows how to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runner {
    Gut,
    GdUnit4,
    Script,
}

impl Runner {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gut => "gut",
            Self::GdUnit4 => "gdunit4",
            Self::Script => "script",
        }
    }
}

impl fmt::Display for Runner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

const GUT_PLUGIN: &str = "addons/gut/plugin.cfg";
const GDUNIT_PLUGIN: &str = "addons/gdUnit4/plugin.cfg";

/// Decides which runner to use: the one in `[test] runner`, else the one whose
/// addon is installed (it is an error to have both), else the script.
pub fn detect_runner(project: &Path, config: &TestConfig) -> Result<Runner, TestError> {
    let has_gut = project.join(GUT_PLUGIN).is_file();
    let has_gdunit = project.join(GDUNIT_PLUGIN).is_file();

    if let Some(name) = &config.runner {
        return match name.to_ascii_lowercase().as_str() {
            "gut" if has_gut => Ok(Runner::Gut),
            "gut" => Err(TestError::RunnerMissing {
                runner: "gut",
                folder: "gut",
            }),
            "gdunit4" | "gdunit" if has_gdunit => Ok(Runner::GdUnit4),
            "gdunit4" | "gdunit" => Err(TestError::RunnerMissing {
                runner: "gdunit4",
                folder: "gdUnit4",
            }),
            "script" => Ok(Runner::Script),
            _ => Err(TestError::UnknownRunner(name.clone())),
        };
    }
    match (has_gut, has_gdunit) {
        (true, true) => Err(TestError::BothRunners),
        (true, false) => Ok(Runner::Gut),
        (false, true) => Ok(Runner::GdUnit4),
        (false, false) if config.script.is_some() => Ok(Runner::Script),
        (false, false) => Err(TestError::NoRunner),
    }
}

/// A folder or file inside the project, as Godot names it (`res://test/unit`).
pub fn res_path(project: &Path, base: &Path, input: &str) -> Result<String, TestError> {
    if let Some(rest) = input.strip_prefix("res://") {
        return Ok(format!("res://{}", rest.trim_matches('/')));
    }
    let joined = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        base.join(input)
    };
    // Resolve `.` and `..` by hand, since the path may not exist yet.
    let mut clean = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    let project_clean: PathBuf = project.components().collect();
    let relative = clean
        .strip_prefix(&project_clean)
        .map_err(|_| TestError::OutsideProject(input.to_string()))?;
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    Ok(if parts.is_empty() {
        "res://".to_string()
    } else {
        format!("res://{}", parts.join("/"))
    })
}

/// What to run and what to report to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestPlan {
    pub runner: Runner,
    /// The arguments for Godot, after the executable.
    pub args: Vec<String>,
    /// Where the runner writes its JUnit report, if it can (a `res://` path inside
    /// `.godot`, which is not committed).
    pub report: Option<PathBuf>,
}

/// The folder (relative to the project) where gdt asks runners to put reports.
const REPORT_DIR: &str = ".godot/gdt-tests";

fn is_file_path(res: &str, project: &Path) -> bool {
    let relative = res.strip_prefix("res://").unwrap_or(res);
    let on_disk = project.join(relative);
    on_disk.is_file() || (!on_disk.exists() && relative.ends_with(".gd"))
}

/// Builds the Godot command line. `paths` are already `res://` paths; empty
/// means the default folder `res://test`.
pub fn build_plan(
    project: &Path,
    runner: Runner,
    config: &TestConfig,
    paths: &[String],
) -> Result<TestPlan, TestError> {
    let project_arg = project.to_string_lossy().to_string();
    let mut args: Vec<String> = vec!["--headless".into(), "--path".into(), project_arg];
    let chosen: Vec<String> = if paths.is_empty() {
        vec!["res://test".to_string()]
    } else {
        paths.to_vec()
    };

    match runner {
        Runner::Gut => {
            let (files, dirs): (Vec<&String>, Vec<&String>) =
                chosen.iter().partition(|p| is_file_path(p, project));
            for dir in &dirs {
                let on_disk = project.join(dir.trim_start_matches("res://"));
                if !on_disk.is_dir() {
                    return Err(TestError::NoTests(dir.to_string()));
                }
            }
            args.extend(["-s".into(), "addons/gut/gut_cmdln.gd".into()]);
            if !dirs.is_empty() {
                let joined: Vec<&str> = dirs.iter().map(|d| d.as_str()).collect();
                args.push(format!("-gdir={}", joined.join(",")));
                args.push("-ginclude_subdirs".into());
            }
            if !files.is_empty() {
                let joined: Vec<&str> = files.iter().map(|f| f.as_str()).collect();
                args.push(format!("-gtest={}", joined.join(",")));
            }
            args.push("-gexit".into());
            args.push(format!("-gjunit_xml_file=res://{REPORT_DIR}/junit.xml"));
            Ok(TestPlan {
                runner,
                args,
                report: Some(project.join(REPORT_DIR).join("junit.xml")),
            })
        }
        Runner::GdUnit4 => {
            // The command line of gdUnit4's own runtest script.
            args.extend([
                "-s".into(),
                "-d".into(),
                "--remote-debug".into(),
                "tcp://127.0.0.1:0".into(),
                "res://addons/gdUnit4/bin/GdUnitCmdTool.gd".into(),
            ]);
            for path in &chosen {
                args.push("--add".into());
                args.push(path.clone());
            }
            args.extend([
                "--ignoreHeadlessMode".into(),
                "--continue".into(),
                "-rd".into(),
                format!("res://{REPORT_DIR}/gdunit4"),
            ]);
            Ok(TestPlan {
                runner,
                args,
                report: None,
            })
        }
        Runner::Script => {
            let script = config.script.as_deref().ok_or(TestError::NoScript)?;
            let res = res_path(project, project, script)?;
            if !project.join(res.trim_start_matches("res://")).is_file() {
                return Err(TestError::ScriptMissing(script.to_string()));
            }
            args.extend(["-s".into(), res]);
            Ok(TestPlan {
                runner,
                args,
                report: None,
            })
        }
    }
}

/// What a JUnit report says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub tests: u32,
    pub failures: u32,
    pub errors: u32,
    pub skipped: u32,
}

impl Summary {
    pub fn passed(&self) -> u32 {
        self.tests
            .saturating_sub(self.failures + self.errors + self.skipped)
    }
}

fn attribute(tag: &str, name: &str) -> Option<u32> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].parse().ok()
}

/// Reads the totals of a JUnit report: from `<testsuites>` when it has them,
/// otherwise by adding up the `<testsuite>` elements.
pub fn parse_junit(xml: &str) -> Option<Summary> {
    /// The opening tags named `name`, without the closing `>`.
    fn tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
        let open = format!("<{name}");
        let mut found = Vec::new();
        for (start, _) in xml.match_indices(&open) {
            // `<testsuite` must not match `<testsuites`.
            if name == "testsuite" && xml[start + open.len()..].starts_with('s') {
                continue;
            }
            if let Some(end) = xml[start..].find('>') {
                found.push(&xml[start..start + end]);
            }
        }
        found
    }
    let read = |tag: &str| Summary {
        tests: attribute(tag, "tests").unwrap_or(0),
        failures: attribute(tag, "failures").unwrap_or(0),
        errors: attribute(tag, "errors").unwrap_or(0),
        skipped: attribute(tag, "skipped").unwrap_or(0),
    };

    if let Some(root) = tags(xml, "testsuites")
        .first()
        .filter(|t| attribute(t, "tests").is_some())
    {
        return Some(read(root));
    }
    let suites = tags(xml, "testsuite");
    if suites.is_empty() {
        return None;
    }
    Some(
        suites
            .iter()
            .map(|t| read(t))
            .fold(Summary::default(), |a, b| Summary {
                tests: a.tests + b.tests,
                failures: a.failures + b.failures,
                errors: a.errors + b.errors,
                skipped: a.skipped + b.skipped,
            }),
    )
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Tests ran and all passed.
    Passed,
    /// At least one test failed or errored.
    Failed,
    /// Nothing ran, so there is nothing to trust.
    NothingRan,
    /// The runner itself failed, with this exit code, before reporting.
    RunnerFailed(i32),
}

/// Decides the outcome from the runner's exit code and its report, if any.
pub fn judge(runner: Runner, exit_code: i32, summary: Option<Summary>) -> Outcome {
    match (runner, summary) {
        (_, Some(s)) if s.failures + s.errors > 0 => Outcome::Failed,
        (_, Some(s)) if s.tests == 0 => Outcome::NothingRan,
        (_, Some(_)) if exit_code != 0 => Outcome::Failed,
        (_, Some(_)) => Outcome::Passed,
        // GUT says "Nothing was run" and exits 0 when it cannot find a test.
        (Runner::Gut, None) if exit_code == 0 => Outcome::NothingRan,
        // A script, or gdUnit4 without a report we read, answers with its exit code.
        (Runner::Script | Runner::GdUnit4, None) if exit_code == 0 => Outcome::Passed,
        (Runner::Script, None) => Outcome::Failed,
        (_, None) => Outcome::RunnerFailed(exit_code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config(runner: Option<&str>, script: Option<&str>) -> TestConfig {
        TestConfig {
            runner: runner.map(str::to_string),
            paths: vec![],
            script: script.map(str::to_string),
        }
    }

    fn install(project: &Path, plugin: &str) {
        let file = project.join(plugin);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, "[plugin]").unwrap();
    }

    #[test]
    fn the_runner_follows_the_installed_addon() {
        let p = scratch("runner-detect");
        assert!(matches!(
            detect_runner(&p, &config(None, None)),
            Err(TestError::NoRunner)
        ));
        assert_eq!(
            detect_runner(&p, &config(None, Some("t.gd"))).unwrap(),
            Runner::Script
        );

        install(&p, GUT_PLUGIN);
        assert_eq!(detect_runner(&p, &config(None, None)).unwrap(), Runner::Gut);
        install(&p, GDUNIT_PLUGIN);
        assert!(matches!(
            detect_runner(&p, &config(None, None)),
            Err(TestError::BothRunners)
        ));
        assert_eq!(
            detect_runner(&p, &config(Some("gdunit4"), None)).unwrap(),
            Runner::GdUnit4
        );
        assert_eq!(
            detect_runner(&p, &config(Some("GUT"), None)).unwrap(),
            Runner::Gut
        );
        assert_eq!(
            detect_runner(&p, &config(Some("script"), None)).unwrap(),
            Runner::Script
        );
        assert!(matches!(
            detect_runner(&p, &config(Some("pytest"), None)),
            Err(TestError::UnknownRunner(_))
        ));
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn a_runner_named_but_not_installed_says_so() {
        let p = scratch("runner-missing");
        let err = detect_runner(&p, &config(Some("gut"), None)).unwrap_err();
        assert!(matches!(
            err,
            TestError::RunnerMissing { runner: "gut", .. }
        ));
        assert!(err.to_string().contains("gdt sync"));
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn paths_become_res_paths() {
        let project = scratch("res-path");
        let sub = project.join("game").join("scripts");
        fs::create_dir_all(&sub).unwrap();
        let r = |base: &Path, input: &str| res_path(&project, base, input);
        assert_eq!(r(&project, "test").unwrap(), "res://test");
        assert_eq!(r(&project, "./test/unit/").unwrap(), "res://test/unit");
        assert_eq!(r(&sub, "../../test").unwrap(), "res://test");
        assert_eq!(r(&project, "res://test/x.gd").unwrap(), "res://test/x.gd");
        assert_eq!(r(&project, ".").unwrap(), "res://");
        let absolute = project.join("test").to_string_lossy().to_string();
        assert_eq!(r(&sub, &absolute).unwrap(), "res://test");
        assert!(matches!(
            r(&project, "../elsewhere"),
            Err(TestError::OutsideProject(_))
        ));
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn gut_gets_its_documented_command_line() {
        let p = scratch("plan-gut");
        fs::create_dir_all(p.join("test").join("unit")).unwrap();
        fs::write(p.join("test").join("unit").join("test_a.gd"), "").unwrap();
        let plan = build_plan(&p, Runner::Gut, &TestConfig::default(), &[]).unwrap();
        assert_eq!(plan.runner, Runner::Gut);
        assert_eq!(&plan.args[..2], ["--headless", "--path"]);
        for expected in [
            "-s",
            "addons/gut/gut_cmdln.gd",
            "-gdir=res://test",
            "-ginclude_subdirs",
            "-gexit",
            "-gjunit_xml_file=res://.godot/gdt-tests/junit.xml",
        ] {
            assert!(
                plan.args.iter().any(|a| a == expected),
                "missing {expected}: {:?}",
                plan.args
            );
        }
        assert_eq!(plan.report, Some(p.join(".godot/gdt-tests/junit.xml")));

        // A file and a folder together.
        let paths = vec![
            "res://test/unit/test_a.gd".to_string(),
            "res://test".to_string(),
        ];
        let plan = build_plan(&p, Runner::Gut, &TestConfig::default(), &paths).unwrap();
        assert!(
            plan.args
                .iter()
                .any(|a| a == "-gtest=res://test/unit/test_a.gd")
        );
        assert!(plan.args.iter().any(|a| a == "-gdir=res://test"));

        let missing = build_plan(
            &p,
            Runner::Gut,
            &TestConfig::default(),
            &["res://nope".into()],
        );
        assert!(matches!(missing, Err(TestError::NoTests(_))));
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn gdunit4_gets_the_command_line_of_its_own_runner() {
        let p = scratch("plan-gdunit");
        let paths = vec!["res://test".to_string(), "res://other".to_string()];
        let plan = build_plan(&p, Runner::GdUnit4, &TestConfig::default(), &paths).unwrap();
        let joined = plan.args.join(" ");
        assert!(
            joined.contains(
                "-s -d --remote-debug tcp://127.0.0.1:0 res://addons/gdUnit4/bin/GdUnitCmdTool.gd"
            ),
            "{joined}"
        );
        assert!(
            joined.contains("--add res://test --add res://other"),
            "{joined}"
        );
        assert!(joined.contains("--ignoreHeadlessMode"), "{joined}");
        assert_eq!(plan.report, None);
        let _ = fs::remove_dir_all(p);
    }

    #[test]
    fn a_test_script_is_run_headless() {
        let p = scratch("plan-script");
        fs::create_dir_all(p.join("tools")).unwrap();
        fs::write(p.join("tools").join("run.gd"), "extends SceneTree").unwrap();
        let cfg = config(Some("script"), Some("tools/run.gd"));
        let plan = build_plan(&p, Runner::Script, &cfg, &[]).unwrap();
        assert_eq!(plan.args[3..], ["-s", "res://tools/run.gd"]);

        assert!(matches!(
            build_plan(&p, Runner::Script, &config(Some("script"), None), &[]),
            Err(TestError::NoScript)
        ));
        assert!(matches!(
            build_plan(
                &p,
                Runner::Script,
                &config(Some("script"), Some("nope.gd")),
                &[]
            ),
            Err(TestError::ScriptMissing(_))
        ));
        let _ = fs::remove_dir_all(p);
    }

    const GUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="GutTests" failures="1" tests="3" >
  <testsuite name="test/unit/test_math.gd" tests="2" failures="1" skipped="0" time="0.000775" >
      <testcase name="test_adds" assertions="1" status="pass" classname="test/unit/test_math.gd" time="0.000476" >
      </testcase>
  </testsuite>
</testsuites>"#;

    #[test]
    fn junit_totals_are_read_from_the_root() {
        let s = parse_junit(GUT_XML).unwrap();
        assert_eq!((s.tests, s.failures, s.errors, s.skipped), (3, 1, 0, 0));
        assert_eq!(s.passed(), 2);
    }

    #[test]
    fn junit_totals_fall_back_to_the_suites() {
        let xml = r#"<testsuites><testsuite name="a" tests="2" failures="0" errors="1" skipped="1"></testsuite><testsuite name="b" tests="3" failures="1"></testsuite></testsuites>"#;
        let s = parse_junit(xml).unwrap();
        assert_eq!((s.tests, s.failures, s.errors, s.skipped), (5, 1, 1, 1));
        let bare = r#"<testsuite name="only" tests="4" failures="0"></testsuite>"#;
        assert_eq!(parse_junit(bare).unwrap().tests, 4);
        assert_eq!(parse_junit("<html>not a report</html>"), None);
        assert_eq!(parse_junit(""), None);
    }

    #[test]
    fn the_outcome_does_not_trust_the_exit_code_alone() {
        let s = |tests, failures| {
            Some(Summary {
                tests,
                failures,
                errors: 0,
                skipped: 0,
            })
        };
        assert_eq!(judge(Runner::Gut, 0, s(3, 0)), Outcome::Passed);
        assert_eq!(judge(Runner::Gut, 1, s(3, 1)), Outcome::Failed);
        assert_eq!(
            judge(Runner::Gut, 0, s(3, 1)),
            Outcome::Failed,
            "failures in the report win over exit 0"
        );
        assert_eq!(judge(Runner::Gut, 1, s(3, 0)), Outcome::Failed);
        assert_eq!(judge(Runner::Gut, 0, s(0, 0)), Outcome::NothingRan);
        // GUT prints "Nothing was run" and exits 0, with no report.
        assert_eq!(judge(Runner::Gut, 0, None), Outcome::NothingRan);
        assert_eq!(judge(Runner::Gut, 1, None), Outcome::RunnerFailed(1));
        assert_eq!(judge(Runner::Script, 0, None), Outcome::Passed);
        assert_eq!(judge(Runner::Script, 3, None), Outcome::Failed);
        assert_eq!(judge(Runner::GdUnit4, 0, None), Outcome::Passed);
        assert_eq!(
            judge(Runner::GdUnit4, 100, None),
            Outcome::RunnerFailed(100)
        );
    }
}
