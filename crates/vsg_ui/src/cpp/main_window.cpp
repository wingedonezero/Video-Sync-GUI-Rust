#include "main_window.h"

#include <QGroupBox>
#include <QHBoxLayout>
#include <QVBoxLayout>

MainWindow::MainWindow(AppController* ctrl, QWidget* parent)
    : QMainWindow(parent), controller(ctrl) {
    setWindowTitle("Video/Audio Sync & Merge - Rust Edition");
    setGeometry(100, 100, 1000, 600);

    setupUi();
    connectSignals();

    // Set initial status
    statusLabel->setText("Ready");
}

void MainWindow::setupUi() {
    // Central widget
    auto* central = new QWidget(this);
    setCentralWidget(central);
    auto* mainLayout = new QVBoxLayout(central);

    // Top row with settings button
    auto* topRow = new QHBoxLayout();
    settingsBtn = new QPushButton("Settings...", this);
    topRow->addWidget(settingsBtn);
    topRow->addStretch();
    mainLayout->addLayout(topRow);

    // Main Workflow group
    auto* actionsGroup = new QGroupBox("Main Workflow", this);
    auto* actionsLayout = new QVBoxLayout(actionsGroup);

    jobQueueBtn = new QPushButton("Open Job Queue for Merging...", this);
    jobQueueBtn->setStyleSheet("font-size: 14px; padding: 5px;");
    actionsLayout->addWidget(jobQueueBtn);

    archiveLogsCheck = new QCheckBox("Archive logs to a zip file on batch completion", this);
    archiveLogsCheck->setChecked(true);
    actionsLayout->addWidget(archiveLogsCheck);

    mainLayout->addWidget(actionsGroup);

    // Quick Analysis group
    auto* analysisGroup = new QGroupBox("Quick Analysis (Analyze Only)", this);
    auto* analysisLayout = new QVBoxLayout(analysisGroup);

    // Source 1
    auto* source1Layout = new QHBoxLayout();
    source1Layout->addWidget(new QLabel("Source 1 (Reference):", this), 1);
    source1Input = new QLineEdit(this);
    source1Layout->addWidget(source1Input, 8);
    source1BrowseBtn = new QPushButton("Browse...", this);
    source1Layout->addWidget(source1BrowseBtn, 1);
    analysisLayout->addLayout(source1Layout);

    // Source 2
    auto* source2Layout = new QHBoxLayout();
    source2Layout->addWidget(new QLabel("Source 2:", this), 1);
    source2Input = new QLineEdit(this);
    source2Layout->addWidget(source2Input, 8);
    source2BrowseBtn = new QPushButton("Browse...", this);
    source2Layout->addWidget(source2BrowseBtn, 1);
    analysisLayout->addLayout(source2Layout);

    // Source 3
    auto* source3Layout = new QHBoxLayout();
    source3Layout->addWidget(new QLabel("Source 3:", this), 1);
    source3Input = new QLineEdit(this);
    source3Layout->addWidget(source3Input, 8);
    source3BrowseBtn = new QPushButton("Browse...", this);
    source3Layout->addWidget(source3BrowseBtn, 1);
    analysisLayout->addLayout(source3Layout);

    // Analyze button (right-aligned)
    auto* analyzeBtnLayout = new QHBoxLayout();
    analyzeBtnLayout->addStretch();
    analyzeBtn = new QPushButton("Analyze Only", this);
    analyzeBtnLayout->addWidget(analyzeBtn);
    analysisLayout->addLayout(analyzeBtnLayout);

    mainLayout->addWidget(analysisGroup);

    // Status row
    auto* statusLayout = new QHBoxLayout();
    statusLayout->addWidget(new QLabel("Status:", this));
    statusLabel = new QLabel("Ready", this);
    statusLayout->addWidget(statusLabel, 1);
    progressBar = new QProgressBar(this);
    progressBar->setRange(0, 100);
    progressBar->setValue(0);
    progressBar->setTextVisible(true);
    statusLayout->addWidget(progressBar);
    mainLayout->addLayout(statusLayout);

    // Latest Job Results group
    auto* resultsGroup = new QGroupBox("Latest Job Results", this);
    auto* resultsLayout = new QHBoxLayout(resultsGroup);
    resultsLayout->addWidget(new QLabel("Source 2 Delay:", this));
    source2DelayLabel = new QLabel("—", this);
    resultsLayout->addWidget(source2DelayLabel);
    resultsLayout->addSpacing(20);
    resultsLayout->addWidget(new QLabel("Source 3 Delay:", this));
    source3DelayLabel = new QLabel("—", this);
    resultsLayout->addWidget(source3DelayLabel);
    resultsLayout->addStretch();
    mainLayout->addWidget(resultsGroup);

    // Log group
    auto* logGroup = new QGroupBox("Log", this);
    auto* logLayout = new QVBoxLayout(logGroup);
    logOutput = new QTextEdit(this);
    logOutput->setReadOnly(true);
    logOutput->setFontFamily("monospace");
    logLayout->addWidget(logOutput);
    mainLayout->addWidget(logGroup);
}

void MainWindow::connectSignals() {
    // Connect browse buttons to controller
    connect(source1BrowseBtn, &QPushButton::clicked, [this]() {
        QString path = controller->browse_file(QString("Select Reference File"));
        if (!path.isEmpty()) {
            source1Input->setText(path);
        }
    });

    connect(source2BrowseBtn, &QPushButton::clicked, [this]() {
        QString path = controller->browse_file(QString("Select Secondary File"));
        if (!path.isEmpty()) {
            source2Input->setText(path);
        }
    });

    connect(source3BrowseBtn, &QPushButton::clicked, [this]() {
        QString path = controller->browse_file(QString("Select Tertiary File"));
        if (!path.isEmpty()) {
            source3Input->setText(path);
        }
    });

    // Connect analyze button
    connect(analyzeBtn, &QPushButton::clicked, [this]() {
        controller->analyze_sources(
            source1Input->text(),
            source2Input->text(),
            source3Input->text()
        );
    });

    // Connect stub buttons
    connect(settingsBtn, &QPushButton::clicked, [this]() {
        controller->open_settings();
    });

    connect(jobQueueBtn, &QPushButton::clicked, [this]() {
        controller->open_job_queue();
    });

    // Connect controller signals to UI slots
    connect(controller, &AppController::log_message, this, &MainWindow::onLogMessage);
    connect(controller, &AppController::progress_update, this, &MainWindow::onProgressUpdate);
    connect(controller, &AppController::status_update, this, &MainWindow::onStatusUpdate);
    connect(controller, &AppController::analysis_complete, this, &MainWindow::onAnalysisComplete);
}

void MainWindow::onLogMessage(const QString& message) {
    logOutput->append(message);
    // Auto-scroll to bottom
    logOutput->verticalScrollBar()->setValue(logOutput->verticalScrollBar()->maximum());
}

void MainWindow::onProgressUpdate(int percent) {
    progressBar->setValue(percent);
}

void MainWindow::onStatusUpdate(const QString& status) {
    statusLabel->setText(status);
}

void MainWindow::onAnalysisComplete(qint64 source2Delay, qint64 source3Delay) {
    source2DelayLabel->setText(QString("%1 ms").arg(source2Delay));
    source3DelayLabel->setText(QString("%1 ms").arg(source3Delay));
}
