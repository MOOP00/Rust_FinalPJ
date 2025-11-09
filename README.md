Task with Me - README

A desktop task scheduler for automating shell command execution at regular intervals. 
Built with Rust and Iced.

Schedule shell commands to run automatically at specified intervals. Monitor execution, view logs, and track success rates.


Features

Automated Scheduling - Run commands every N seconds
Task Management - Create, start, pause, delete tasks
Execution Logs - View output and errors from each run
Quick Templates - Pre-configured tasks for common operations
Cross-platform - Windows, macOS, Linux support
Persistent Storage - Tasks saved between sessions

Create task with command and interval
Click "Start" - task becomes active
Background checker runs every 5 seconds (configurable)
When interval elapsed, command executes
Output captured and logged
Next run scheduled automatically
Continue until "Pause" clicked

Main NavigationFour tabs at the top right:

Overview - Dashboard and statistics:

Total Tasks: How many tasks you've created
Active: Tasks currently scheduled to run automatically
Running: Tasks executing right now
Success Rate: Overall percentage of successful executions
New Task: Jump to Tasks screen
View All Tasks: Open Tasks screen
View Logs: Open Logs screen
Task name
Success rate percentage
Last execution time
Button to view task-specific logs


Tasks - Create and manage tasks:

Title (Task Name)
Command (Shell Command)
Interval (Time Interval)

Create Button:

Click to save the task
Validates all fields before saving
Shows notification on success/error
Form clears after successful creation
Task appears in list below immediately

Quick Templates

Search and Filter Controls



Logs - Execution history

View detailed execution logs with output, errors, and timing information.

All Logs View (Default):
Shows logs from all tasks
Most recent first (newest on top)
Up to 50 entries displayed
No filter applied

Task-Specific View (When clicking Logs button from task):
Shows only logs for one task
Filtered by task ID
"View All Logs" button to return to unfiltered view
Task name displayed in header


Settings - Configure application

Configure application behavior and appearance:
Configure refresh interval (task checking frequency)
Set max log entries (history limit)
Choose theme (light/dark)
Save changes to disk
Validation on inputs