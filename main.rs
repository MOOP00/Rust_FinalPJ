use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{alignment, executor, time, window, Application, Command, Element, Length, Settings, Subscription, Theme, Color, Font};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use chrono::{DateTime, Local};
use uuid::Uuid;

//Error Handling
#[derive(Debug, Clone)]
pub enum AppError {
    Io(String),
    Serialization(String),
    Config(String),
    Execution(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Io(msg) => write!(f, "I/O error: {}", msg),
            AppError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            AppError::Config(msg) => write!(f, "Configuration error: {}", msg),
            AppError::Execution(msg) => write!(f, "Execution error: {}", msg),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization(err.to_string())
    }
}

//Data Structures
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: Uuid,
    title: String,
    command: String,
    interval_seconds: u64,
    is_active: bool,
    last_run: Option<DateTime<Local>>,
    next_run: Option<DateTime<Local>>,
    created_at: DateTime<Local>,
    success_count: u32,
    failure_count: u32,
    #[serde(skip)]
    last_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionLog {
    id: Uuid,
    task_id: Uuid,
    timestamp: DateTime<Local>,
    success: bool,
    output: String,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    refresh_interval: u64,
    max_logs: usize,
    theme: AppTheme,
    log_to_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum AppTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone)]
struct TaskTemplate {
    name: &'static str,
    description: &'static str,
    command: &'static str,
    interval: u64,
}

#[derive(Debug, Clone)]
struct Notification {
    id: Uuid,
    message: String,
    level: NotificationLevel,
    timestamp: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq)]
enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

//Messages
#[derive(Debug, Clone)]
enum Message {
    // Navigation
    ChangeScreen(Screen),
    
    // Task Management
    TitleInput(String),
    CommandInput(String),
    IntervalInput(String),
    CreateTask,
    DeleteTask(Uuid),
    ToggleTask(Uuid),
    ExecuteTask(Uuid),
    
    // Async Results
    TasksLoaded(Result<Vec<Task>, AppError>),
    LogsLoaded(Result<Vec<ExecutionLog>, AppError>),
    TaskSaved(Result<(), AppError>),
    TaskExecuted(Uuid, Result<ExecutionResult, AppError>),
    TaskDeleted(Result<(), AppError>),
    ConfigLoaded(Result<Config, AppError>),
    ConfigSaved(Result<(), AppError>),
    
    // UI Actions
    SelectTemplate(usize),
    SearchInput(String),
    FilterChanged(TaskFilter),
    ViewTaskLogs(Uuid),
    CloseNotification(Uuid),
    ClearNotifications,
    
    // Settings
    ThemeChanged(AppTheme),
    RefreshIntervalChanged(String),
    MaxLogsChanged(String),
    SaveSettings,
    
    // Background
    Tick,
    CheckScheduledTasks,
}

#[derive(Debug, Clone)]
struct ExecutionResult {
    success: bool,
    output: String,
    duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
enum TaskFilter {
    All,
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Overview,
    Tasks,
    Logs(Option<Uuid>),
    Settings,
}

//Application State
struct TaskWithMe {
    // Core data
    tasks: Vec<Task>,
    logs: Vec<ExecutionLog>,
    config: Config,
    
    // UI state
    screen: Screen,
    title_input: String,
    command_input: String,
    interval_input: String,
    search_query: String,
    filter: TaskFilter,
    
    // Runtime state
    notifications: VecDeque<Notification>,
    running_tasks: Vec<Uuid>,
    last_check: Instant,
    
    // Settings inputs
    refresh_input: String,
    max_logs_input: String,
    
    // Templates
    templates: Vec<TaskTemplate>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_interval: 5,
            max_logs: 500,
            theme: AppTheme::Dark,
            log_to_file: true,
        }
    }
}

impl Default for TaskWithMe {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            logs: Vec::new(),
            config: Config::default(),
            screen: Screen::Overview,
            title_input: String::new(),
            command_input: String::new(),
            interval_input: String::new(),
            search_query: String::new(),
            filter: TaskFilter::All,
            notifications: VecDeque::new(),
            running_tasks: Vec::new(),
            last_check: Instant::now(),
            refresh_input: "5".to_string(),
            max_logs_input: "500".to_string(),
            templates: get_templates(),
        }
    }
}

fn get_templates() -> Vec<TaskTemplate> {
    vec![
        TaskTemplate {
            name: "System Cleanup",
            description: "Remove temp files",
            command: if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
                "find /tmp -name '*.tmp' -mtime +7 -delete"
            } else {
                "del /q /s %TEMP%\\*.tmp"
            },
            interval: 3600,
        },
        TaskTemplate {
            name: "Backup Documents",
            description: "Create backup archive",
            command: if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
                "tar -czf ~/backups/docs-$(date +%Y%m%d).tar.gz ~/Documents"
            } else {
                "echo Backup complete"
            },
            interval: 86400,
        },
        TaskTemplate {
            name: "Check Disk Space",
            description: "Monitor disk usage",
            command: if cfg!(target_os = "windows") {
                 "wmic logicaldisk get size,freespace" 
                } else 
                { "df -h" 
            },
            interval: 300,
        },
        TaskTemplate {
            name: "Health Ping",
            description: "Test network connectivity",
            command: if cfg!(target_os = "windows") {
                "ping -n 4 8.8.8.8" 
            } else { 
                "ping -c 4 8.8.8.8" 
            },
            interval: 60,
        },
    ]
}

impl TaskWithMe {
    fn notify(&mut self, message: String, level: NotificationLevel) {
        let notification = Notification {
            id: Uuid::new_v4(),
            message,
            level,
            timestamp: Local::now(),
        };
        
        self.notifications.push_back(notification);
        if self.notifications.len() > 10 {
            self.notifications.pop_front();
        }
    }
    
    fn filtered_tasks(&self) -> Vec<&Task> {
        self.tasks.iter()
            .filter(|task| {
                let matches_search = self.search_query.is_empty() ||
                    task.title.to_lowercase().contains(&self.search_query.to_lowercase()) ||
                    task.command.to_lowercase().contains(&self.search_query.to_lowercase());
                
                let matches_filter = match self.filter {
                    TaskFilter::All => true,
                    TaskFilter::Active => task.is_active,
                    TaskFilter::Inactive => !task.is_active,
                };
                
                matches_search && matches_filter
            })
            .collect()
    }
    
    fn format_duration(seconds: u64) -> String {
        if seconds < 60 {
            format!("{}s", seconds)
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else if seconds < 86400 {
            format!("{}h", seconds / 3600)
        } else {
            format!("{}d", seconds / 86400)
        }
    }
    
    fn success_rate(&self, task: &Task) -> f32 {
        let total = task.success_count + task.failure_count;
        if total == 0 {
            0.0
        } else {
            (task.success_count as f32 / total as f32) * 100.0
        }
    }
}

//Application Implement
impl Application for TaskWithMe {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let mut app = TaskWithMe::default();
        app.refresh_input = app.config.refresh_interval.to_string();
        app.max_logs_input = app.config.max_logs.to_string();
        
        let load_config = Command::perform(load_config(), Message::ConfigLoaded);
        let load_tasks = Command::perform(load_tasks(), Message::TasksLoaded);
        let load_logs = Command::perform(load_logs(), Message::LogsLoaded);
        
        (app, Command::batch(vec![load_config, load_tasks, load_logs]))
    }

    fn title(&self) -> String {
        match &self.screen {
            Screen::Overview => "Overview - Task with Me".to_string(),
            Screen::Tasks => "Tasks - Task with Me".to_string(),
            Screen::Logs(_) => "Logs - Task with Me".to_string(),
            Screen::Settings => "Settings - Task with Me".to_string(),
        }
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::ChangeScreen(screen) => {
                self.screen = screen;
                Command::none()
            }
            
            Message::TitleInput(s) => {
                self.title_input = s;
                Command::none()
            }
            
            Message::CommandInput(s) => {
                self.command_input = s;
                Command::none()
            }
            
            Message::IntervalInput(s) => {
                self.interval_input = s;
                Command::none()
            }
            
            Message::CreateTask => {
                if self.title_input.trim().is_empty() {
                    self.notify("Task title cannot be empty".to_string(), NotificationLevel::Warning);
                    return Command::none();
                }
                
                if self.command_input.trim().is_empty() {
                    self.notify("Command cannot be empty".to_string(), NotificationLevel::Warning);
                    return Command::none();
                }
                
                let interval = match self.interval_input.parse::<u64>() {
                    Ok(n) if n > 0 => n,
                    _ => {
                        self.notify("Invalid interval".to_string(), NotificationLevel::Warning);
                        return Command::none();
                    }
                };
                
                let task = Task {
                    id: Uuid::new_v4(),
                    title: std::mem::take(&mut self.title_input),
                    command: std::mem::take(&mut self.command_input),
                    interval_seconds: interval,
                    is_active: false,
                    last_run: None,
                    next_run: None,
                    created_at: Local::now(),
                    success_count: 0,
                    failure_count: 0,
                    last_output: String::new(),
                };
                
                self.interval_input.clear();
                
                println!("Creating task: {} (ID: {})", task.title, task.id);
                self.notify(format!("Task '{}' created", task.title), NotificationLevel::Success);
                
                Command::perform(save_task(task), Message::TaskSaved)
            }
            
            Message::DeleteTask(id) => {
                if let Some(task) = self.tasks.iter().find(|t| t.id == id) {
                    self.notify(format!("Deleted task '{}'", task.title), NotificationLevel::Info);
                }
                Command::perform(delete_task(id), Message::TaskDeleted)
            }
            
            Message::ToggleTask(id) => {
                let mut task_to_save = None;
                let mut notification_msg = String::new();
                
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    task.is_active = !task.is_active;
                    if task.is_active {
                        task.next_run = Some(Local::now() + chrono::Duration::seconds(task.interval_seconds as i64));
                    } else {
                        task.next_run = None;
                    }
                    
                    let status = if task.is_active { "activated" } else { "paused" };
                    notification_msg = format!("Task '{}' {}", task.title, status);
                    task_to_save = Some(task.clone());
                }
                
                if let Some(task) = task_to_save {
                    self.notify(notification_msg, NotificationLevel::Info);
                    return Command::perform(save_task(task), Message::TaskSaved);
                }
                Command::none()
            }
            
            Message::ExecuteTask(id) => {
                if self.running_tasks.contains(&id) {
                    self.notify("Task is already running".to_string(), NotificationLevel::Warning);
                    return Command::none();
                }
                
                let task_info = self.tasks.iter().find(|t| t.id == id).map(|task| {
                    (task.clone(), task.title.clone())
                });
                
                if let Some((task_clone, task_title)) = task_info {
                    self.running_tasks.push(id);
                    self.notify(format!("Executing '{}'...", task_title), NotificationLevel::Info);
                    
                    return Command::perform(
                        execute_task(task_clone),
                        move |result| Message::TaskExecuted(id, result)
                    );
                }
                Command::none()
            }
            
            Message::TaskExecuted(id, result) => {
                self.running_tasks.retain(|&tid| tid != id);
                
                let mut commands = vec![];
                
                match result {
                    Ok(exec_result) => {
                        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                            task.last_run = Some(Local::now());
                            task.last_output = exec_result.output.clone();
                            
                            let success = exec_result.success;
                            let task_title = task.title.clone();
                            
                            if success {
                                task.success_count += 1;
                            } else {
                                task.failure_count += 1;
                            }
                            
                            if task.is_active {
                                task.next_run = Some(Local::now() + chrono::Duration::seconds(task.interval_seconds as i64));
                            }
                            
                            let log = ExecutionLog {
                                id: Uuid::new_v4(),
                                task_id: id,
                                timestamp: Local::now(),
                                success: exec_result.success,
                                output: exec_result.output,
                                duration_ms: exec_result.duration_ms,
                            };
                            
                            self.logs.push(log);
                            if self.logs.len() > self.config.max_logs {
                                self.logs.remove(0);
                            }
                            
                            let task_clone = task.clone();
                            let logs_clone = self.logs.clone();
                            
                            if success {
                                self.notify(
                                    format!("Task '{}' completed successfully", task_title),
                                    NotificationLevel::Success
                                );
                            } else {
                                self.notify(
                                    format!("Task '{}' failed", task_title),
                                    NotificationLevel::Error
                                );
                            }
                            
                            commands.push(Command::perform(save_task(task_clone), Message::TaskSaved));
                            commands.push(Command::perform(save_logs(logs_clone), |_| Message::Tick));
                        }
                    }
                    Err(e) => {
                        self.notify(format!("Execution error: {}", e), NotificationLevel::Error);
                    }
                }
                Command::batch(commands)
            }
            
            Message::TasksLoaded(Ok(tasks)) => {
                println!("Tasks loaded: {} tasks", tasks.len());
                for task in &tasks {
                    println!("  - {} ({})", task.title, task.id);
                }
                self.tasks = tasks;
                Command::none()
            }
            
            Message::TasksLoaded(Err(e)) => {
                self.notify(format!("Failed to load tasks: {}", e), NotificationLevel::Error);
                Command::none()
            }
            
            Message::LogsLoaded(Ok(logs)) => {
                self.logs = logs;
                Command::none()
            }
            
            Message::LogsLoaded(Err(_)) => {
                Command::none()
            }
            
            Message::TaskSaved(Ok(())) => {
                println!("Task saved successfully, reloading...");
                Command::perform(load_tasks(), Message::TasksLoaded)
            }
            
            Message::TaskSaved(Err(e)) => {
                self.notify(format!("Failed to save: {}", e), NotificationLevel::Error);
                Command::none()
            }
            
            Message::TaskDeleted(Ok(())) => {
                Command::perform(load_tasks(), Message::TasksLoaded)
            }
            
            Message::TaskDeleted(Err(e)) => {
                self.notify(format!("Failed to delete: {}", e), NotificationLevel::Error);
                Command::none()
            }
            
            Message::ConfigLoaded(Ok(config)) => {
                self.config = config;
                self.refresh_input = self.config.refresh_interval.to_string();
                self.max_logs_input = self.config.max_logs.to_string();
                Command::none()
            }
            
            Message::ConfigLoaded(Err(_)) => {
                Command::none()
            }
            
            Message::ConfigSaved(Ok(())) => {
                self.notify("Settings saved".to_string(), NotificationLevel::Success);
                self.screen = Screen::Overview;
                Command::none()
            }
            
            Message::ConfigSaved(Err(e)) => {
                self.notify(format!("Failed to save settings: {}", e), NotificationLevel::Error);
                Command::none()
            }
            
            Message::SelectTemplate(idx) => {
                if let Some(template) = self.templates.get(idx) {
                    self.title_input = template.name.to_string();
                    self.command_input = template.command.to_string();
                    self.interval_input = template.interval.to_string();
                    self.notify(format!("Template loaded: {}", template.name), NotificationLevel::Info);
                }
                Command::none()
            }
            
            Message::SearchInput(s) => {
                self.search_query = s;
                Command::none()
            }
            
            Message::FilterChanged(filter) => {
                self.filter = filter;
                Command::none()
            }
            
            Message::ViewTaskLogs(id) => {
                self.screen = Screen::Logs(Some(id));
                Command::none()
            }
            
            Message::CloseNotification(id) => {
                self.notifications.retain(|n| n.id != id);
                Command::none()
            }
            
            Message::ClearNotifications => {
                self.notifications.clear();
                Command::none()
            }
            
            Message::ThemeChanged(theme) => {
                self.config.theme = theme;
                Command::none()
            }
            
            Message::RefreshIntervalChanged(s) => {
                self.refresh_input = s;
                Command::none()
            }
            
            Message::MaxLogsChanged(s) => {
                self.max_logs_input = s;
                Command::none()
            }
            
            Message::SaveSettings => {
                if let Ok(interval) = self.refresh_input.parse::<u64>() {
                    self.config.refresh_interval = interval.max(1);
                }
                if let Ok(max_logs) = self.max_logs_input.parse::<usize>() {
                    self.config.max_logs = max_logs.max(10);
                }
                
                Command::perform(save_config(self.config.clone()), Message::ConfigSaved)
            }
            
            Message::Tick => Command::none(),
            
            Message::CheckScheduledTasks => {
                let now = Local::now();
                let mut commands = vec![];
                
                for task in &self.tasks {
                    if task.is_active {
                        if let Some(next_run) = task.next_run {
                            if now >= next_run && !self.running_tasks.contains(&task.id) {
                                let task_id = task.id;
                                commands.push(Command::perform(
                                    async move { task_id },
                                    Message::ExecuteTask
                                ));
                            }
                        }
                    }
                }
                
                Command::batch(commands)
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let content = match &self.screen {
            Screen::Overview => self.view_overview(),
            Screen::Tasks => self.view_tasks(),
            Screen::Logs(task_id) => self.view_logs(*task_id),
            Screen::Settings => self.view_settings(),
        };

        column![
            self.view_header(),
            Space::with_height(20),
            content,
            self.view_notifications(),
        ]
        .padding(20)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        time::every(Duration::from_secs(self.config.refresh_interval))
            .map(|_| Message::CheckScheduledTasks)
    }

    fn theme(&self) -> Theme {
        match self.config.theme {
            AppTheme::Light => Theme::Light,
            AppTheme::Dark => Theme::Dark,
        }
    }
}

//View Components
impl TaskWithMe {
    fn view_header(&self) -> Element<Message> {
        let nav_button = |label: &str, screen: Screen, is_active: bool| {
            button(text(label).size(14))
                .on_press(Message::ChangeScreen(screen))
                .padding([10, 16])
                .style(if is_active {
                    iced::theme::Button::Primary
                } else {
                    iced::theme::Button::Secondary
                })
        };

        container(
            row![
                text("[Task with Me]").size(22),
                Space::with_width(Length::Fill),
                row![
                    nav_button("Overview", Screen::Overview, 
                        matches!(self.screen, Screen::Overview)),
                    nav_button("Tasks", Screen::Tasks, 
                        matches!(self.screen, Screen::Tasks)),
                    nav_button("Logs", Screen::Logs(None), 
                        matches!(self.screen, Screen::Logs(_))),
                    nav_button("Settings", Screen::Settings, 
                        matches!(self.screen, Screen::Settings)),
                ]
                .spacing(8),
            ]
            .align_items(alignment::Alignment::Center)
        )
        .width(Length::Fill)
        .into()
    }
    
    fn view_overview(&self) -> Element<Message> {
        let total = self.tasks.len();
        let active = self.tasks.iter().filter(|t| t.is_active).count();
        let running = self.running_tasks.len();
        
        let total_success: u32 = self.tasks.iter().map(|t| t.success_count).sum();
        let total_failure: u32 = self.tasks.iter().map(|t| t.failure_count).sum();
        let total_runs = total_success + total_failure;
        let success_rate = if total_runs > 0 {
            (total_success as f32 / total_runs as f32 * 100.0) as usize
        } else {
            0
        };
        
        let stats = row![
            self.stat_card("Total Tasks", total, Color::from_rgb(0.2, 0.6, 0.9)),
            self.stat_card("Active", active, Color::from_rgb(0.3, 0.8, 0.4)),
            self.stat_card("Running", running, Color::from_rgb(0.95, 0.7, 0.2)),
            self.stat_card("Success Rate", success_rate, Color::from_rgb(0.7, 0.4, 0.9)),
        ]
        .spacing(15);
        
        let quick_actions = container(
            column![
                text("Quick Actions").size(18),
                Space::with_height(15),
                row![
                    button("+ New Task")
                        .on_press(Message::ChangeScreen(Screen::Tasks))
                        .padding(15)
                        .style(iced::theme::Button::Primary),
                    button("View All Tasks")
                        .on_press(Message::ChangeScreen(Screen::Tasks))
                        .padding(15),
                    button("View Logs")
                        .on_press(Message::ChangeScreen(Screen::Logs(None)))
                        .padding(15),
                ]
                .spacing(10),
            ]
        )
        .padding(20)
        .style(iced::theme::Container::Box);
        
        let recent_tasks = self.view_recent_tasks();
        
        column![
            text("Dashboard").size(26),
            Space::with_height(20),
            stats,
            Space::with_height(25),
            quick_actions,
            Space::with_height(25),
            recent_tasks,
        ]
        .into()
    }
    
    fn stat_card(&self, label: &str, value: usize, color: Color) -> Element<Message> {
        let display = if label == "Success Rate" {
            format!("{}%", value)
        } else {
            value.to_string()
        };
        
        container(
            column![
                text(label).size(13),
                Space::with_height(8),
                text(display).size(24).style(color),
            ]
        )
        .padding(20)
        .width(Length::FillPortion(1))
        .style(iced::theme::Container::Box)
        .into()
    }
    
    fn view_recent_tasks(&self) -> Element<Message> {
        let mut recent: Vec<&Task> = self.tasks.iter()
            .filter(|t| t.last_run.is_some())
            .collect();
        recent.sort_by(|a, b| b.last_run.cmp(&a.last_run));
        recent.truncate(5);
        
        let content: Element<Message> = if recent.is_empty() {
            container(text("No tasks have run yet").size(14))
                .center_x()
                .padding(40)
                .into()
        } else {
            let mut list = column![].spacing(8);
            
            for task in recent {
                let success_rate = self.success_rate(task);
                let card = container(
                    row![
                        column![
                            text(&task.title).size(14),
                            text(format!("Last run: {}", 
                                task.last_run.unwrap().format("%b %d, %H:%M")))
                                .size(11),
                        ]
                        .width(Length::Fill),
                        text(format!("{:.0}%", success_rate)).size(13),
                        button("View Logs")
                            .on_press(Message::ViewTaskLogs(task.id))
                            .padding(8)
                            .style(iced::theme::Button::Secondary),
                    ]
                    .align_items(alignment::Alignment::Center)
                    .spacing(12)
                )
                .padding(12)
                .style(iced::theme::Container::Box);
                
                list = list.push(card);
            }
            
            scrollable(list).height(Length::Fixed(250.0)).into()
        };
        
        container(
            column![
                text("Recent Activity").size(18),
                Space::with_height(12),
                content,
            ]
        )
        .padding(20)
        .style(iced::theme::Container::Box)
        .into()
    }
    
    fn view_tasks(&self) -> Element<Message> {
        // Task creation form
        let form = container(
            column![
                text("Create New Task").size(18),
                Space::with_height(12),
                row![
                    column![
                        text("Title").size(12),
                        text_input("Enter task title", &self.title_input)
                            .on_input(Message::TitleInput)
                            .padding(8)
                            .width(Length::Fixed(200.0)),
                    ]
                    .spacing(4),
                    column![
                        text("Command").size(12),
                        text_input("Enter shell command", &self.command_input)
                            .on_input(Message::CommandInput)
                            .padding(8)
                            .width(Length::Fixed(300.0)),
                    ]
                    .spacing(4),
                    column![
                        text("Interval (sec)").size(12),
                        text_input("60", &self.interval_input)
                            .on_input(Message::IntervalInput)
                            .padding(8)
                            .width(Length::Fixed(120.0)),
                    ]
                    .spacing(4),
                    column![
                        Space::with_height(12),
                        button("Create")
                            .on_press(Message::CreateTask)
                            .padding(8)
                            .style(iced::theme::Button::Primary),
                    ],
                ]
                .spacing(10)
                .align_items(alignment::Alignment::End),
            ]
        )
        .padding(20)
        .style(iced::theme::Container::Box);
        
        // Templates
        let mut templates_col = column![
            text("Quick Templates").size(16),
            Space::with_height(10),
        ].spacing(6);
        
        for (idx, template) in self.templates.iter().enumerate() {
            let btn = button(
                column![
                    text(template.name).size(13),
                    text(template.description).size(11),
                ]
                .spacing(2)
            )
            .on_press(Message::SelectTemplate(idx))
            .padding(10)
            .width(Length::Fill)
            .style(iced::theme::Button::Secondary);
            
            templates_col = templates_col.push(btn);
        }
        
        let templates = container(templates_col)
            .padding(15)
            .style(iced::theme::Container::Box);
        
        // Search and filter
        let controls = container(
            row![
                text_input("Search tasks...", &self.search_query)
                    .on_input(Message::SearchInput)
                    .padding(8)
                    .width(Length::Fixed(250.0)),
                Space::with_width(Length::Fill),
                row![
                    button("All")
                        .on_press(Message::FilterChanged(TaskFilter::All))
                        .style(if self.filter == TaskFilter::All {
                            iced::theme::Button::Primary
                        } else {
                            iced::theme::Button::Secondary
                        })
                        .padding([6, 12]),
                    button("Active")
                        .on_press(Message::FilterChanged(TaskFilter::Active))
                        .style(if self.filter == TaskFilter::Active {
                            iced::theme::Button::Primary
                        } else {
                            iced::theme::Button::Secondary
                        })
                        .padding([6, 12]),
                    button("Inactive")
                        .on_press(Message::FilterChanged(TaskFilter::Inactive))
                        .style(if self.filter == TaskFilter::Inactive {
                            iced::theme::Button::Primary
                        } else {
                            iced::theme::Button::Secondary
                        })
                        .padding([6, 12]),
                ]
                .spacing(6),
            ]
            .align_items(alignment::Alignment::Center)
        )
        .padding(12)
        .style(iced::theme::Container::Box);
        
        // Task list
        let filtered = self.filtered_tasks();
        
        // Debug info
        let debug_info = container(
            text(format!("Total tasks: {} | Filtered: {} | Active filter: {:?}", 
                self.tasks.len(), filtered.len(), self.filter))
                .size(11)
        )
        .padding(8)
        .style(iced::theme::Container::Box);
        
        let task_list: Element<Message> = if filtered.is_empty() {
            container(
                column![
                    text("No tasks found").size(14),
                    Space::with_height(10),
                    text(format!("Total tasks in memory: {}", self.tasks.len())).size(11),
                ]
            )
            .center_x()
            .padding(40)
            .into()
        } else {
            let mut list = column![].spacing(8);
            
            println!("Rendering {} tasks", filtered.len());
            
            for task in filtered {
                let is_running = self.running_tasks.contains(&task.id);
                let success_rate = self.success_rate(task);
                
                let status_color = if task.is_active {
                    Color::from_rgb(0.3, 0.8, 0.4)
                } else {
                    Color::from_rgb(0.5, 0.5, 0.5)
                };
                
                println!("  Rendering task: {}", task.title);
                
                let card = container(
                    row![
                        container(Space::with_width(4))
                            .width(Length::Fixed(4.0))
                            .height(Length::Fixed(80.0))
                            .style(iced::theme::Container::Custom(Box::new(
                                ColoredContainer(status_color)
                            ))),
                        column![
                            row![
                                text(&task.title).size(15),
                                Space::with_width(Length::Fill),
                                text(format!("{:.0}%", success_rate)).size(12),
                            ]
                            .align_items(alignment::Alignment::Center),
                            text(&task.command).size(12),
                            row![
                                text(format!("Every {}", Self::format_duration(task.interval_seconds)))
                                    .size(11),
                                Space::with_width(Length::Fill),
                                if let Some(next) = task.next_run {
                                    text(format!("Next: {}", next.format("%H:%M"))).size(11)
                                } else {
                                    text("Not scheduled").size(11)
                                },
                            ],
                        ]
                        .spacing(4)
                        .width(Length::Fill),
                        row![
                            button(if is_running { "Running" } else { "Run" })
                                .on_press(Message::ExecuteTask(task.id))
                                .padding(8)
                                .style(if is_running {
                                    iced::theme::Button::Secondary
                                } else {
                                    iced::theme::Button::Primary
                                }),
                            button(if task.is_active { "Pause" } else { "Start" })
                                .on_press(Message::ToggleTask(task.id))
                                .padding(8)
                                .style(iced::theme::Button::Secondary),
                            button("Logs")
                                .on_press(Message::ViewTaskLogs(task.id))
                                .padding(8)
                                .style(iced::theme::Button::Secondary),
                            button("Delete")
                                .on_press(Message::DeleteTask(task.id))
                                .padding(8)
                                .style(iced::theme::Button::Destructive),
                        ]
                        .spacing(6),
                    ]
                    .align_items(alignment::Alignment::Center)
                    .spacing(12)
                )
                .padding(12)
                .width(Length::Fill)
                .style(iced::theme::Container::Box);
                
                list = list.push(card);
            }
            
            container(
                scrollable(list)
                    .height(Length::Fill)
            )
            .height(Length::Fixed(400.0))
            .into()
        };
        
        column![
            text("Task Management").size(26),
            Space::with_height(20),
            form,
            Space::with_height(15),
            templates,
            Space::with_height(15),
            controls,
            Space::with_height(12),
            debug_info,
            Space::with_height(8),
            task_list,
        ]
        .into()
    }
    
    fn view_logs(&self, task_id: Option<Uuid>) -> Element<Message> {
        let filtered_logs: Vec<&ExecutionLog> = if let Some(id) = task_id {
            self.logs.iter().filter(|l| l.task_id == id).collect()
        } else {
            self.logs.iter().collect()
        };
        
        let task_name = task_id.and_then(|id| {
            self.tasks.iter().find(|t| t.id == id).map(|t| t.title.clone())
        });
        
        let header = if let Some(name) = task_name {
            text(format!("Logs for: {}", name)).size(20)
        } else {
            text("All Execution Logs").size(20)
        };
        
        let content: Element<Message> = if filtered_logs.is_empty() {
            container(text("No logs available").size(14))
                .center_x()
                .padding(40)
                .into()
        } else {
            let mut list = column![].spacing(8);
            
            for log in filtered_logs.iter().rev().take(50) {
                let task_title = self.tasks.iter()
                    .find(|t| t.id == log.task_id)
                    .map(|t| t.title.as_str())
                    .unwrap_or("Unknown");
                
                let status_color = if log.success {
                    Color::from_rgb(0.3, 0.8, 0.4)
                } else {
                    Color::from_rgb(0.9, 0.3, 0.3)
                };
                
                let card = container(
                    column![
                        row![
                            container(
                                text(if log.success { "OK" } else { "FAIL" })
                                    .size(14)
                            )
                            .padding([4, 8])
                            .style(iced::theme::Container::Custom(Box::new(
                                ColoredContainer(status_color)
                            ))),
                            text(task_title).size(14),
                            Space::with_width(Length::Fill),
                            text(log.timestamp.format("%b %d, %H:%M:%S").to_string())
                                .size(12),
                            text(format!("{}ms", log.duration_ms)).size(11),
                        ]
                        .align_items(alignment::Alignment::Center)
                        .spacing(10),
                        if !log.output.is_empty() {
                            container(
                                text(&log.output).size(11)
                            )
                            .padding([8, 12])
                            .style(iced::theme::Container::Box)
                        } else {
                            container(Space::with_height(0))
                        },
                    ]
                    .spacing(8)
                )
                .padding(12)
                .style(iced::theme::Container::Box);
                
                list = list.push(card);
            }
            
            scrollable(list).height(Length::Fixed(500.0)).into()
        };
        
        column![
            text("Execution Logs").size(26),
            Space::with_height(20),
            container(
                row![
                    header,
                    Space::with_width(Length::Fill),
                    if task_id.is_some() {
                        button("View All Logs")
                            .on_press(Message::ChangeScreen(Screen::Logs(None)))
                            .padding(8)
                    } else {
                        button("")
                            .padding(0)
                            .style(iced::theme::Button::Text)
                    },
                ]
                .align_items(alignment::Alignment::Center)
            )
            .padding(15)
            .style(iced::theme::Container::Box),
            Space::with_height(12),
            content,
        ]
        .into()
    }
    
    fn view_settings(&self) -> Element<Message> {
        column![
            text("Settings").size(26),
            Space::with_height(20),
            container(
                column![
                    text("General").size(18),
                    Space::with_height(15),
                    row![
                        text("Refresh Interval (seconds):").size(14).width(Length::Fixed(200.0)),
                        text_input("5", &self.refresh_input)
                            .on_input(Message::RefreshIntervalChanged)
                            .padding(8)
                            .width(Length::Fixed(100.0)),
                    ]
                    .align_items(alignment::Alignment::Center)
                    .spacing(10),
                    Space::with_height(12),
                    row![
                        text("Max Log Entries:").size(14).width(Length::Fixed(200.0)),
                        text_input("500", &self.max_logs_input)
                            .on_input(Message::MaxLogsChanged)
                            .padding(8)
                            .width(Length::Fixed(100.0)),
                    ]
                    .align_items(alignment::Alignment::Center)
                    .spacing(10),
                ]
            )
            .padding(20)
            .style(iced::theme::Container::Box),
            Space::with_height(20),
            container(
                column![
                    text("Appearance").size(18),
                    Space::with_height(15),
                    row![
                        button("Light Theme")
                            .on_press(Message::ThemeChanged(AppTheme::Light))
                            .style(if self.config.theme == AppTheme::Light {
                                iced::theme::Button::Primary
                            } else {
                                iced::theme::Button::Secondary
                            })
                            .padding(10),
                        button("Dark Theme")
                            .on_press(Message::ThemeChanged(AppTheme::Dark))
                            .style(if self.config.theme == AppTheme::Dark {
                                iced::theme::Button::Primary
                            } else {
                                iced::theme::Button::Secondary
                            })
                            .padding(10),
                    ]
                    .spacing(10),
                ]
            )
            .padding(20)
            .style(iced::theme::Container::Box),
            Space::with_height(20),
            button("Save Settings")
                .on_press(Message::SaveSettings)
                .padding(12)
                .style(iced::theme::Button::Primary),
        ]
        .into()
    }
    
    fn view_notifications(&self) -> Element<Message> {
        if self.notifications.is_empty() {
            return Space::with_height(0).into();
        }
        
        let mut list = column![].spacing(8);
        
        for notif in self.notifications.iter().rev() {
            let (label, color) = match notif.level {
                NotificationLevel::Info => ("INFO", Color::from_rgb(0.2, 0.6, 0.9)),
                NotificationLevel::Success => ("SUCCESS", Color::from_rgb(0.3, 0.8, 0.4)),
                NotificationLevel::Warning => ("WARNING", Color::from_rgb(0.95, 0.7, 0.2)),
                NotificationLevel::Error => ("ERROR", Color::from_rgb(0.9, 0.3, 0.3)),
            };
            
            let card = container(
                row![
                    container(text(label).size(11))
                        .padding([4, 8])
                        .style(iced::theme::Container::Custom(Box::new(
                            ColoredContainer(color)
                        ))),
                    text(&notif.message).size(13).width(Length::Fill),
                    button("X")
                        .on_press(Message::CloseNotification(notif.id))
                        .padding(6)
                        .style(iced::theme::Button::Destructive),
                ]
                .align_items(alignment::Alignment::Center)
                .spacing(10)
            )
            .padding(12)
            .style(iced::theme::Container::Box);
            
            list = list.push(card);
        }
        
        container(list)
            .width(Length::Fill)
            .padding([0, 0, 15, 0])
            .into()
    }
}

//Custom Container Style
struct ColoredContainer(Color);

impl iced::widget::container::StyleSheet for ColoredContainer {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> iced::widget::container::Appearance {
        iced::widget::container::Appearance {
            background: Some(iced::Background::Color(self.0)),
            ..Default::default()
        }
    }
}

//Storage Functions
fn get_data_dir() -> Result<PathBuf, AppError> {
    let dir = dirs::data_local_dir()
        .ok_or_else(|| AppError::Config("Cannot determine data directory".to_string()))?
        .join("task-with-me");
    
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

async fn load_config() -> Result<Config, AppError> {
    let path = get_data_dir()?.join("config.json");
    
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        let config = Config::default();
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(&path, content)?;
        Ok(config)
    }
}

async fn save_config(config: Config) -> Result<(), AppError> {
    let path = get_data_dir()?.join("config.json");
    let content = serde_json::to_string_pretty(&config)?;
    fs::write(&path, content)?;
    Ok(())
}

async fn load_tasks() -> Result<Vec<Task>, AppError> {
    let path = get_data_dir()?.join("tasks.json");
    
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(Vec::new())
    }
}

async fn load_logs() -> Result<Vec<ExecutionLog>, AppError> {
    let path = get_data_dir()?.join("logs.json");
    
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(Vec::new())
    }
}

async fn save_task(task: Task) -> Result<(), AppError> {
    let path = get_data_dir()?.join("tasks.json");
    
    let mut tasks: Vec<Task> = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    
    if let Some(pos) = tasks.iter().position(|t| t.id == task.id) {
        tasks[pos] = task;
    } else {
        tasks.push(task);
    }
    
    let content = serde_json::to_string_pretty(&tasks)?;
    fs::write(&path, content)?;
    Ok(())
}

async fn delete_task(id: Uuid) -> Result<(), AppError> {
    let path = get_data_dir()?.join("tasks.json");
    
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        let mut tasks: Vec<Task> = serde_json::from_str(&content)?;
        tasks.retain(|t| t.id != id);
        let content = serde_json::to_string_pretty(&tasks)?;
        fs::write(&path, content)?;
    }
    
    Ok(())
}

async fn save_logs(logs: Vec<ExecutionLog>) -> Result<(), AppError> {
    let path = get_data_dir()?.join("logs.json");
    let content = serde_json::to_string_pretty(&logs)?;
    fs::write(&path, content)?;
    Ok(())
}

async fn execute_task(task: Task) -> Result<ExecutionResult, AppError> {
    let start = Instant::now();
    
    let (shell, flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    
    let output = tokio::process::Command::new(shell)
        .arg(flag)
        .arg(&task.command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Execution(e.to_string()))?;
    
    let duration = start.elapsed();
    let success = output.status.success();
    
    let output_text = if success {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    };
    
    let result = ExecutionResult {
        success,
        output: output_text,
        duration_ms: duration.as_millis() as u64,
    };
    
    Ok(result)
}

// Main
fn main() -> iced::Result {
    TaskWithMe::run(Settings {
        window: window::Settings {
            size: iced::Size::new(1200.0, 800.0),
            min_size: Some(iced::Size::new(900.0, 600.0)),
            ..Default::default()
        },
        default_font: Font::default(),
        ..Settings::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task {
            id: Uuid::new_v4(),
            title: "Test".to_string(),
            command: "echo test".to_string(),
            interval_seconds: 60,
            is_active: false,
            last_run: None,
            next_run: None,
            created_at: Local::now(),
            success_count: 0,
            failure_count: 0,
            last_output: String::new(),
        };
        
        assert_eq!(task.title, "Test");
        assert!(!task.is_active);
    }

    #[test]
    fn test_success_rate_calculation() {
        let app = TaskWithMe::default();
        let mut task = Task {
            id: Uuid::new_v4(),
            title: "Test".to_string(),
            command: "test".to_string(),
            interval_seconds: 60,
            is_active: false,
            last_run: None,
            next_run: None,
            created_at: Local::now(),
            success_count: 7,
            failure_count: 3,
            last_output: String::new(),
        };
        
        assert_eq!(app.success_rate(&task), 70.0);
        
        task.success_count = 0;
        task.failure_count = 0;
        assert_eq!(app.success_rate(&task), 0.0);
    }
    
    #[test]
    fn test_duration_formatting() {
        assert_eq!(TaskWithMe::format_duration(45), "45s");
        assert_eq!(TaskWithMe::format_duration(120), "2m");
        assert_eq!(TaskWithMe::format_duration(7200), "2h");
        assert_eq!(TaskWithMe::format_duration(172800), "2d");
    }
}