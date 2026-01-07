#![windows_subsystem = "windows"]
use iced::{executor, Alignment, Application, Command, Element, Length, Settings, Theme};
use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input, vertical_space};
use iced::Color;
use std::{fs, path::PathBuf, time::SystemTime};

// --- CONFIGURATION STRUCTS ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    pub scan_filter: String,
    pub unit: Unit,
    #[serde(default)]
    pub default_path: String,
    // NEW: Save the default sort method
    #[serde(default)]
    pub default_sort: SortMethod,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            scan_filter: "*".to_string(),
            unit: Unit::MB,
            default_path: String::new(),
            default_sort: SortMethod::NameAZ,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Unit {
    KB,
    MB,
    GB,
}

impl Unit {
    fn convert(&self, bytes: u64) -> f64 {
        match self {
            Unit::KB => bytes as f64 / 1024.0,
            Unit::MB => bytes as f64 / (1024.0 * 1024.0),
            Unit::GB => bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        }
    }
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Unit::KB => "KB",
                Unit::MB => "MB",
                Unit::GB => "GB",
            }
        )
    }
}

// --- SORTING ENUM ---
// Added Serialize/Deserialize here so we can save it to JSON
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SortMethod {
    #[default]
    NameAZ,
    NameZA,
    SizeLargest,
    SizeSmallest,
    Newest,
    Oldest,
}

impl std::fmt::Display for SortMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SortMethod::NameAZ => "Name (A-Z)",
                SortMethod::NameZA => "Name (Z-A)",
                SortMethod::SizeLargest => "Size (Largest First)",
                SortMethod::SizeSmallest => "Size (Smallest First)",
                SortMethod::Newest => "Date (Newest First)",
                SortMethod::Oldest => "Date (Oldest First)",
            }
        )
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: u64,
}

// --- HELPER FUNCTIONS ---

fn get_config_path() -> Result<PathBuf, anyhow::Error> {
    let config_dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
    let app_dir = config_dir.join("disk-maid-rs");
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir)?;
    }
    Ok(app_dir.join("settings.json"))
}

fn load_config() -> Result<AppConfig, anyhow::Error> {
    let path = get_config_path()?;
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    } else {
        Ok(AppConfig::default())
    }
}

fn save_config(config: &AppConfig) -> Result<(), anyhow::Error> {
    let path = get_config_path()?;
    let content = serde_json::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}

fn scan_directory(path: PathBuf, filter: String) -> Result<Vec<FileInfo>, String> {
    let mut files = Vec::new();

    fn scan_recursive(dir: &PathBuf, filter: &str, files: &mut Vec<FileInfo>, depth: usize, max_depth: usize) -> Result<(), String> {
        if depth > max_depth {
            return Ok(());
        }
        if files.len() > 10000 {
            return Ok(());
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                let metadata = match fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let path_str = path.to_string_lossy().to_string();
                
                let modified = metadata.modified()
                    .unwrap_or(SystemTime::UNIX_EPOCH)
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if metadata.is_dir() {
                    files.push(FileInfo {
                        path: path_str.clone(),
                        size: 0,
                        is_dir: true,
                        modified,
                    });
                    let _ = scan_recursive(&path, filter, files, depth + 1, max_depth);
                } else {
                    let matches = if filter == "*" || filter == "*.*" {
                        true
                    } else if filter.starts_with("*.") {
                        let ext = filter.trim_start_matches("*.");
                        path.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case(ext))
                            .unwrap_or(false)
                    } else {
                        true
                    };

                    if matches {
                        files.push(FileInfo {
                            path: path_str,
                            size: metadata.len(),
                            is_dir: false,
                            modified,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    scan_recursive(&path, &filter, &mut files, 0, 5)?;
    Ok(files)
}

fn sort_files(files: &mut Vec<FileInfo>, method: SortMethod) {
    files.sort_by(|a, b| match method {
        SortMethod::NameAZ => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
        SortMethod::NameZA => b.path.to_lowercase().cmp(&a.path.to_lowercase()),
        SortMethod::SizeLargest => b.size.cmp(&a.size),
        SortMethod::SizeSmallest => a.size.cmp(&b.size),
        SortMethod::Newest => b.modified.cmp(&a.modified),
        SortMethod::Oldest => a.modified.cmp(&b.modified),
    });
}

// --- CUSTOM THEME & STYLING ---

#[derive(Default)]
pub struct CustomTheme;

#[derive(Debug, Clone, Copy, Default)]
pub enum ContainerStyle {
    #[default]
    Base,
    RowOdd,
    RowEven,
}

impl From<ContainerStyle> for iced::theme::Container {
    fn from(style: ContainerStyle) -> Self {
        iced::theme::Container::Custom(Box::new(move |_theme: &Theme| {
            match style {
                ContainerStyle::Base => container::Appearance {
                    background: Some(iced::Background::Color(Color::from_rgb8(10, 40, 70))),
                    border: iced::Border {
                        radius: 5.0.into(),
                        width: 1.0,
                        color: Color::from_rgb8(0, 100, 150),
                    },
                    text_color: Some(Color::from_rgb8(255, 255, 255)),
                    shadow: iced::Shadow::default(),
                },
                ContainerStyle::RowOdd => container::Appearance {
                    background: Some(iced::Background::Color(Color::from_rgb8(45, 45, 45))),
                    text_color: Some(Color::from_rgb8(255, 255, 255)),
                    ..Default::default()
                },
                ContainerStyle::RowEven => container::Appearance {
                    background: Some(iced::Background::Color(Color::from_rgb8(30, 30, 30))),
                    text_color: Some(Color::from_rgb8(255, 255, 255)),
                    ..Default::default()
                },
            }
        }))
    }
}

// --- MAIN ENTRY POINT ---

pub fn main() -> iced::Result {
    DiskViz::run(Settings::default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    FileScan,
    Settings,
    Help,
}

#[derive(Debug)]
pub struct DiskViz {
    current_screen: Screen,
    config: AppConfig,
    status_message: String,
    
    // Buffers for Settings Screen
    scan_filter_buffer: String,
    default_path_buffer: String, 
    selected_unit: Unit,
    settings_default_sort: SortMethod, // NEW: Buffer for sorting choice in settings

    is_scanning: bool,
    scanned_files: Vec<FileInfo>,
    scan_path_buffer: String,
    pending_delete_file: Option<String>,
    current_sort: SortMethod,
}

#[derive(Debug, Clone)]
pub enum Message {
    ScreenChanged(Screen),
    StartScanPressed,
    StopScanPressed,
    BackToMainMenu,
    ExitApp,
    ScanPathChanged(String),
    ScanCompleted(Result<Vec<FileInfo>, String>),
    ScanFilterChanged(String),
    UnitChanged(Unit),
    SaveSettingsPressed,
    ConfigSaved(Result<(), String>),
    RequestDelete(String),
    ConfirmDelete,
    CancelDelete,
    FileDeleted(Result<String, String>),
    OpenFolder(String),
    BrowseScanPathPressed,
    ScanPathSelected(Option<String>),
    BrowseDefaultPathPressed,
    DefaultPathSelected(Option<String>),
    DefaultPathChanged(String),
    SortChanged(SortMethod), 
    // NEW: Update the buffer in Settings screen
    SettingsDefaultSortChanged(SortMethod), 
}

impl Application for DiskViz {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = load_config().unwrap_or_default();
        
        let home_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        let initial_path = if config.default_path.is_empty() {
            home_dir.clone()
        } else {
            config.default_path.clone()
        };

        (
            DiskViz {
                current_screen: Screen::MainMenu,
                // Initialize Settings buffers
                scan_filter_buffer: config.scan_filter.clone(),
                default_path_buffer: config.default_path.clone(),
                selected_unit: config.unit,
                settings_default_sort: config.default_sort, // Load default sort to buffer

                config: config.clone(),
                is_scanning: false,
                status_message: format!("Welcome! Ready to scan: {}", initial_path),
                scanned_files: Vec::new(),
                scan_path_buffer: initial_path,
                pending_delete_file: None,
                current_sort: config.default_sort, // Apply default sort on startup
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Disk Maid".to_string()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::ScreenChanged(screen) => {
                self.current_screen = screen;
                self.pending_delete_file = None;
                Command::none()
            }

            Message::BackToMainMenu => {
                self.current_screen = Screen::MainMenu;
                self.status_message = "Welcome to Disk Maid!".into();
                Command::none()
            }

            Message::ExitApp => {
                std::process::exit(0);
            }

            Message::BrowseScanPathPressed => {
                Command::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .set_title("Select Directory to Scan")
                            .pick_folder()
                            .await;
                        handle.map(|h| h.path().to_string_lossy().to_string())
                    },
                    Message::ScanPathSelected
                )
            }

            Message::ScanPathSelected(Some(path)) => {
                self.scan_path_buffer = path;
                Command::none()
            }

            Message::ScanPathSelected(None) => Command::none(),

            Message::BrowseDefaultPathPressed => {
                Command::perform(
                    async {
                        let handle = rfd::AsyncFileDialog::new()
                            .set_title("Select Default Directory")
                            .pick_folder()
                            .await;
                        handle.map(|h| h.path().to_string_lossy().to_string())
                    },
                    Message::DefaultPathSelected
                )
            }

            Message::DefaultPathSelected(Some(path)) => {
                self.default_path_buffer = path;
                Command::none()
            }
            
            Message::DefaultPathSelected(None) => Command::none(),

            Message::DefaultPathChanged(val) => {
                self.default_path_buffer = val;
                Command::none()
            }

            Message::StartScanPressed => {
                let path = PathBuf::from(self.scan_path_buffer.clone());
                if !path.exists() {
                    self.status_message = "Error: Path does not exist!".into();
                    return Command::none();
                }
                if !path.is_dir() {
                    self.status_message = "Error: Path is not a directory!".into();
                    return Command::none();
                }

                self.is_scanning = true;
                self.status_message = "Scanning... (limited to 10,000 files)".into();
                self.scanned_files.clear();
                self.pending_delete_file = None;

                let filter = self.config.scan_filter.clone();

                Command::perform(
                    async move {
                        scan_directory(path, filter)
                    },
                    Message::ScanCompleted
                )
            }

            Message::StopScanPressed => {
                self.is_scanning = false;
                self.status_message = "Scan stopped.".into();
                Command::none()
            }

            Message::ScanPathChanged(path) => {
                self.scan_path_buffer = path;
                Command::none()
            }

            Message::ScanCompleted(Ok(mut files)) => {
                self.is_scanning = false;
                sort_files(&mut files, self.current_sort);

                let file_count = files.iter().filter(|f| !f.is_dir).count();
                let dir_count = files.iter().filter(|f| f.is_dir).count();
                let total_size: u64 = files.iter().filter(|f| !f.is_dir).map(|f| f.size).sum();

                self.scanned_files = files;
                self.status_message = format!(
                    "Scan complete! {} files, {} dirs. Size: {:.2} {}",
                    file_count,
                    dir_count,
                    self.config.unit.convert(total_size),
                    self.config.unit
                );
                Command::none()
            }

            Message::ScanCompleted(Err(e)) => {
                self.is_scanning = false;
                self.status_message = format!("Scan error: {}", e);
                Command::none()
            }

            // Sort changed in the Scan View (Temporary)
            Message::SortChanged(method) => {
                self.current_sort = method;
                sort_files(&mut self.scanned_files, self.current_sort);
                Command::none()
            }

            // Sort changed in the Settings View (Buffer)
            Message::SettingsDefaultSortChanged(method) => {
                self.settings_default_sort = method;
                Command::none()
            }

            Message::ScanFilterChanged(new_filter) => {
                self.scan_filter_buffer = new_filter;
                Command::none()
            }

            Message::UnitChanged(unit) => {
                self.selected_unit = unit;
                Command::none()
            }

            Message::SaveSettingsPressed => {
                self.config.scan_filter = self.scan_filter_buffer.clone();
                self.config.unit = self.selected_unit;
                self.config.default_path = self.default_path_buffer.clone();
                
                // Save Default Sort
                self.config.default_sort = self.settings_default_sort;
                
                // Also update current sort immediately to match new default
                self.current_sort = self.settings_default_sort;

                let config_to_save = self.config.clone();

                self.status_message = "Saving settings...".into();

                Command::perform(
                    async move { save_config(&config_to_save).map_err(|e| e.to_string()) },
                    Message::ConfigSaved
                )
            }

            Message::ConfigSaved(Ok(())) => {
                self.status_message = "Settings saved successfully!".into();
                Command::none()
            }

            Message::ConfigSaved(Err(e)) => {
                self.status_message = format!("Error saving settings: {}", e);
                Command::none()
            }

            Message::RequestDelete(path_str) => {
                self.pending_delete_file = Some(path_str);
                self.status_message = "Waiting for confirmation...".into();
                Command::none()
            }

            Message::CancelDelete => {
                self.pending_delete_file = None;
                self.status_message = "Deletion cancelled.".into();
                Command::none()
            }

            Message::ConfirmDelete => {
                if let Some(path_str) = &self.pending_delete_file {
                    let p = path_str.clone();
                    self.status_message = format!("Deleting {}...", p);
                    self.pending_delete_file = None;

                    Command::perform(
                        async move {
                            fs::remove_file(&p).map_err(|e| e.to_string())?;
                            Ok(p)
                        },
                        Message::FileDeleted
                    )
                } else {
                    Command::none()
                }
            }

            Message::FileDeleted(Ok(path)) => {
                if let Some(index) = self.scanned_files.iter().position(|x| x.path == path) {
                    self.scanned_files.remove(index);
                }
                self.status_message = format!("Successfully deleted: {}", path);
                Command::none()
            }

            Message::FileDeleted(Err(e)) => {
                self.status_message = format!("Failed to delete file: {}", e);
                Command::none()
            }

            Message::OpenFolder(path_str) => {
                let path = PathBuf::from(&path_str);
                if let Some(parent) = path.parent() {
                    let _ = open::that(parent);
                    self.status_message = format!("Opened folder for: {}", path_str);
                } else {
                    let _ = open::that(path);
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let content = match self.current_screen {
            Screen::MainMenu => main_menu_view(),
            Screen::FileScan => file_scan_view(
                self.is_scanning,
                &self.scan_path_buffer,
                &self.scanned_files,
                self.config.unit,
                &self.pending_delete_file,
                self.current_sort,
            ),
            Screen::Settings => settings_view(
                &self.scan_filter_buffer,
                self.selected_unit,
                &self.default_path_buffer,
                self.settings_default_sort, // Pass the buffer
            ),
            Screen::Help => help_view(),
        };

        let layout = if self.current_screen != Screen::MainMenu {
            column![
                button(text("Back to Main Menu")).on_press(Message::BackToMainMenu),
                vertical_space().height(10),
                content,
                vertical_space(),
                container(text(&self.status_message))
                    .padding(10)
                    .width(Length::Fill)
                    .style(ContainerStyle::Base)
            ]
            .padding(20)
            .align_items(Alignment::Start)
        } else {
            column![content]
                .padding(20)
                .align_items(Alignment::Center)
        };

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

// --- VIEW FUNCTIONS ---

fn main_menu_view() -> Element<'static, Message> {
    let file_scan_btn = button(text("File & Scan").size(20))
        .on_press(Message::ScreenChanged(Screen::FileScan))
        .padding(20)
        .width(Length::Fixed(300.0));

    let settings_btn = button(text("Settings").size(20))
        .on_press(Message::ScreenChanged(Screen::Settings))
        .padding(20)
        .width(Length::Fixed(300.0));

    let help_btn = button(text("Help").size(20))
        .on_press(Message::ScreenChanged(Screen::Help))
        .padding(20)
        .width(Length::Fixed(300.0));

    let exit_btn = button(text("Exit").size(20))
        .on_press(Message::ExitApp)
        .padding(20)
        .width(Length::Fixed(300.0))
        .style(iced::theme::Button::Destructive);

    column![
        text("Disk Maid").size(36),
        file_scan_btn,
        settings_btn,
        help_btn,
        exit_btn,
    ]
    .spacing(20)
    .align_items(Alignment::Center)
    .into()
}

fn file_scan_view<'a>(
    is_scanning: bool,
    scan_path: &'a str,
    files: &'a [FileInfo],
    unit: Unit,
    pending_delete: &'a Option<String>,
    current_sort: SortMethod,
) -> Element<'a, Message> {
    
    let input_row = row![
        text_input("Enter path (e.g., /home/user or C:\\Users)", scan_path)
            .on_input(Message::ScanPathChanged)
            .padding(10)
            .width(Length::Fill),
        button(text("📂 Browse"))
            .on_press(Message::BrowseScanPathPressed)
            .padding(10)
            .style(iced::theme::Button::Secondary)
    ]
    .spacing(10);

    let mut col = column![
        text("File & Scan").size(28),
        text("Select directory to scan:"),
        input_row,
    ];

    let mut controls_row = row![].spacing(20).align_items(Alignment::Center);

    if !is_scanning {
        controls_row = controls_row.push(button(text("▶ Start Scan")).on_press(Message::StartScanPressed).padding(10));
    } else {
        controls_row = controls_row.push(button(text("⏹ Stop Scan")).on_press(Message::StopScanPressed).padding(10));
        controls_row = controls_row.push(text("Scanning..."));
    }

    controls_row = controls_row.push(text("Sort By:"));
    controls_row = controls_row.push(
        pick_list(
            vec![
                SortMethod::NameAZ,
                SortMethod::NameZA,
                SortMethod::SizeLargest,
                SortMethod::SizeSmallest,
                SortMethod::Newest,
                SortMethod::Oldest,
            ],
            Some(current_sort),
            Message::SortChanged
        )
        .width(Length::Fixed(180.0))
    );

    col = col.push(controls_row);

    if !files.is_empty() {
        col = col.push(text(format!("Found {} items:", files.len())).size(18));

        let mut file_list = column![].spacing(0);

        for (i, file) in files.iter().take(200).enumerate() {
            let info_text = if file.is_dir {
                format!("[DIR] {}", file.path)
            } else {
                format!(
                    "{:.2} {} - {}",
                    unit.convert(file.size),
                    unit,
                    file.path
                )
            };

            let mut row_item = row![
                text(info_text).size(12).width(Length::Fill),
            ]
            .spacing(10)
            .align_items(Alignment::Center);

            if !file.is_dir {
                let is_pending_this = pending_delete.as_ref() == Some(&file.path);

                if is_pending_this {
                    row_item = row_item.push(text("Are you sure?").size(12));

                    row_item = row_item.push(
                        button(text("Yes, Delete").size(12))
                            .on_press(Message::ConfirmDelete)
                            .style(iced::theme::Button::Destructive)
                            .padding(5)
                    );

                    row_item = row_item.push(
                        button(text("Cancel").size(12))
                            .on_press(Message::CancelDelete)
                            .style(iced::theme::Button::Secondary)
                            .padding(5)
                    );
                } else {
                    row_item = row_item.push(
                        button(text("Go to Folder").size(12))
                            .on_press(Message::OpenFolder(file.path.clone()))
                            .style(iced::theme::Button::Secondary)
                            .padding(5)
                    );

                    row_item = row_item.push(
                        button(text("Delete").size(12))
                            .on_press(Message::RequestDelete(file.path.clone()))
                            .style(iced::theme::Button::Destructive)
                            .padding(5)
                    );
                }
            }

            let row_style = if i % 2 == 0 {
                ContainerStyle::RowEven
            } else {
                ContainerStyle::RowOdd
            };

            file_list = file_list.push(
                container(row_item)
                    .width(Length::Fill)
                    .padding(5)
                    .style(row_style)
            );
        }

        if files.len() > 200 {
            file_list = file_list.push(text(format!("... and {} more items", files.len() - 200)));
        }

        col = col.push(
            container(scrollable(file_list).height(Length::Fixed(400.0)))
                .style(ContainerStyle::Base)
                .padding(5)
        );
    }

    col.spacing(15).into()
}

fn settings_view<'a>(
    filter: &'a str, 
    unit: Unit, 
    default_path: &'a str,
    default_sort: SortMethod,
) -> Element<'a, Message> {
    
    let path_input = row![
        text_input("Leave empty for Home", default_path)
            .on_input(Message::DefaultPathChanged)
            .padding(10)
            .width(Length::Fill),
        button(text("📂 Browse"))
            .on_press(Message::BrowseDefaultPathPressed)
            .padding(10)
            .style(iced::theme::Button::Secondary)
    ].spacing(10);

    column![
        text("Settings").size(28),
        
        text("Scan Filter:"),
        text_input("e.g., *.txt or *", filter).on_input(Message::ScanFilterChanged),
        
        text("Default Path:"),
        path_input,

        text("Display Unit:"),
        pick_list(
            vec![Unit::KB, Unit::MB, Unit::GB],
            Some(unit),
            Message::UnitChanged
        ),
        
        // NEW: Default Sort Picker
        text("Default Sort Order:"),
        pick_list(
            vec![
                SortMethod::NameAZ,
                SortMethod::NameZA,
                SortMethod::SizeLargest,
                SortMethod::SizeSmallest,
                SortMethod::Newest,
                SortMethod::Oldest,
            ],
            Some(default_sort),
            Message::SettingsDefaultSortChanged
        ),

        vertical_space().height(20),
        
        button(text("Save Settings"))
            .on_press(Message::SaveSettingsPressed)
            .padding(10)
    ]
    .spacing(15)
    .into()
}

fn help_view() -> Element<'static, Message> {
    column![
        text("Help & About").size(28),
        text("How to Use:").size(20),
        text("1. Click 'File & Scan' from the main menu").size(16),
        text("2. Click '📂 Browse' or type a path manually").size(16),
        text("3. Click 'Start Scan'").size(16),
        text("4. Use 'Sort By' to organize files").size(16),
        text("5. Click 'Go to Folder' to open location").size(16),
        text("6. Click 'Delete' -> 'Yes' to remove").size(16),
        vertical_space().height(20),
        text("Settings:").size(20),
        text("• Set a 'Default Path' to auto-load").size(16),
        text("• Set 'Default Sort Order' for consistent listing").size(16),
        text("• Change filters and units").size(16),
        vertical_space().height(20),
        text("About:").size(20),
        text("Disk Maid v2.5.0").size(16),
        text("Now with Default Sorting!").size(16),
    ]
    .spacing(10)
    .into()
}