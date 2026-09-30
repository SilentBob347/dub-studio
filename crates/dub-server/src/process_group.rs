//! Всё, что запускает студия, умирает вместе со студией.
//!
//! Деструкторы сайдкаров (llama-server, bs_roformer-cli, whisper, ffmpeg, openrouter-helper) не
//! срабатывают, когда процесс снимают диспетчером задач, `taskkill /F`, падением или закрытием окна
//! при живом рабочем потоке. Единственный честный ответ Windows — job object с
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: процесс кладёт в него себя, все дети наследуют job при
//! создании, и когда процесс заканчивается как угодно, ядро закрывает последний хендл и гасит всю группу.

use std::process::Command;

/// Кладёт этот процесс и всё, что он запустит дальше, в одну группу с kill-on-close.
///
/// Неудача не фатальна: процесс внутри чужого job, где вложение запрещено, просто живёт по-старому
/// (дети гасятся своими деструкторами).
#[cfg(windows)]
pub fn bind_children_to_this_process() -> bool {
    use std::mem::size_of;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    if JOB.get().is_some() {
        return true;
    }
    unsafe {
        let job: HANDLE = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return false;
        }

        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        // BREAKAWAY_OK сам никого не выпускает: из группы выходит только процесс, запущенный с
        // CREATE_BREAKAWAY_FROM_JOB (см. detach_from_group).
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        let assigned = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0
            && AssignProcessToJobObject(job, GetCurrentProcess()) != 0;

        // Хендл намеренно не закрывается: job должен жить до конца процесса и умереть вместе с ним,
        // что и делает закрытие последнего хендла на выходе.
        if assigned {
            let _ = JOB.set(job as isize);
        }
        assigned
    }
}

#[cfg(not(windows))]
pub fn bind_children_to_this_process() -> bool {
    false
}

#[cfg(windows)]
static JOB: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

/// Разрешает процессам, запущенным с этого момента, пережить этот.
///
/// Установщик обновления запускается этим процессом прямо перед выходом; в группе kill-on-close он
/// умер бы вместе со студией, ничего не установив.
#[cfg(windows)]
pub fn release_children() -> bool {
    use std::mem::size_of;

    use windows_sys::Win32::System::JobObjects::{
        JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    };

    let Some(job) = JOB.get() else { return false };
    unsafe {
        let limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        SetInformationJobObject(
            *job as _,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0
    }
}

#[cfg(not(windows))]
pub fn release_children() -> bool {
    false
}

/// Запускаемое наружу для пользователя (системный плеер, окно проводника) не должно закрываться
/// вместе со студией. Флаг ставится только когда группа своя: в чужом job без BREAKAWAY_OK
/// CreateProcess с ним падает с отказом в доступе.
#[cfg(windows)]
pub fn detach_from_group(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_BREAKAWAY_FROM_JOB;

    if JOB.get().is_some() {
        cmd.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    cmd
}

#[cfg(not(windows))]
pub fn detach_from_group(cmd: &mut Command) -> &mut Command {
    cmd
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use std::process::{Child, Stdio};
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::JobObjects::{
        IsProcessInJob, JobObjectExtendedLimitInformation, QueryInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };

    const OWNER_ENV: &str = "DUB_PROCESS_GROUP_OWNER";

    fn long_child(detached: bool) -> Child {
        let mut cmd = Command::new("ping");
        cmd.args(["-n", "60", "127.0.0.1"]).stdout(Stdio::null()).stderr(Stdio::null());
        if detached {
            detach_from_group(&mut cmd);
        }
        cmd.spawn().expect("запуск ping")
    }

    fn in_our_job(child: &Child) -> bool {
        let job = *JOB.get().expect("группа создана");
        let mut result = 0;
        let ok = unsafe { IsProcessInJob(child.as_raw_handle() as _, job as _, &mut result) };
        assert_ne!(ok, 0, "IsProcessInJob: {}", std::io::Error::last_os_error());
        result != 0
    }

    fn limit_flags() -> u32 {
        let job = *JOB.get().expect("группа создана");
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                job as _,
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "QueryInformationJobObject: {}", std::io::Error::last_os_error());
        info.BasicLimitInformation.LimitFlags
    }

    #[test]
    fn children_inherit_the_group_unless_detached_and_release_lifts_the_kill() {
        assert!(bind_children_to_this_process(), "процесс тестов не встал в свой job");
        let flags = limit_flags();
        assert_ne!(flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, 0);
        assert_ne!(flags & JOB_OBJECT_LIMIT_BREAKAWAY_OK, 0);

        let mut inherited = long_child(false);
        let mut detached = long_child(true);
        let inherited_in = in_our_job(&inherited);
        let detached_in = in_our_job(&detached);
        for c in [&mut inherited, &mut detached] {
            c.kill().ok();
            c.wait().ok();
        }
        assert!(inherited_in, "обычный потомок должен наследовать группу");
        assert!(!detached_in, "detach_from_group должен выводить процесс из группы");

        assert!(release_children());
        assert_eq!(limit_flags() & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, 0);
    }

    /// Владелец группы для a_killed_owner_takes_its_children_down: запускается им как отдельный
    /// процесс (этот же тестовый exe), сам по себе ничего не делает.
    #[test]
    #[ignore = "запускается из a_killed_owner_takes_its_children_down"]
    fn group_owner() {
        if std::env::var_os(OWNER_ENV).is_none() {
            return;
        }
        assert!(bind_children_to_this_process());
        let mut child = long_child(false);
        println!("GRANDCHILD={}", child.id());
        std::thread::sleep(std::time::Duration::from_secs(60));
        child.kill().ok();
        child.wait().ok();
    }

    #[test]
    fn a_killed_owner_takes_its_children_down() {
        let exe = std::env::current_exe().expect("путь тестового exe");
        let mut owner = Command::new(exe)
            .args(["process_group::tests::group_owner", "--exact", "--ignored", "--nocapture"])
            .env(OWNER_ENV, "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("запуск владельца группы");
        let out = owner.stdout.take().expect("stdout владельца");
        let pid = BufReader::new(out)
            .lines()
            .map_while(Result::ok)
            .find_map(|l| l.strip_prefix("GRANDCHILD=").and_then(|p| p.trim().parse::<u32>().ok()))
            .expect("владелец не сообщил pid потомка");
        // Хендл открывается до убийства владельца: pid потомка не успеет достаться другому процессу.
        let grandchild =
            unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        assert!(!grandchild.is_null(), "OpenProcess: {}", std::io::Error::last_os_error());

        owner.kill().expect("убить владельца");
        owner.wait().ok();
        let waited = unsafe { WaitForSingleObject(grandchild, 10_000) };
        unsafe { CloseHandle(grandchild) };
        assert_eq!(waited, WAIT_OBJECT_0, "потомок пережил убитого владельца группы");
    }
}
