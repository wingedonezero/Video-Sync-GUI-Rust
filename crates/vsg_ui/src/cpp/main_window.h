#pragma once

#include <QCheckBox>
#include <QGroupBox>
#include <QLabel>
#include <QLineEdit>
#include <QMainWindow>
#include <QProgressBar>
#include <QPushButton>
#include <QScrollBar>
#include <QTextEdit>

// Include the generated CXX-Qt AppController header
#include "vsg_ui/src/bridge/app_controller.cxxqt.h"

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(AppController* controller, QWidget* parent = nullptr);
    ~MainWindow() override = default;

private slots:
    void onLogMessage(const QString& message);
    void onProgressUpdate(int percent);
    void onStatusUpdate(const QString& status);
    void onAnalysisComplete(qint64 source2Delay, qint64 source3Delay);

private:
    void setupUi();
    void connectSignals();

    // UI Elements
    QLineEdit* source1Input;
    QLineEdit* source2Input;
    QLineEdit* source3Input;
    QPushButton* source1BrowseBtn;
    QPushButton* source2BrowseBtn;
    QPushButton* source3BrowseBtn;
    QPushButton* analyzeBtn;
    QPushButton* settingsBtn;
    QPushButton* jobQueueBtn;
    QCheckBox* archiveLogsCheck;
    QTextEdit* logOutput;
    QProgressBar* progressBar;
    QLabel* statusLabel;
    QLabel* source2DelayLabel;
    QLabel* source3DelayLabel;

    // Controller (Rust bridge)
    AppController* controller;
};
