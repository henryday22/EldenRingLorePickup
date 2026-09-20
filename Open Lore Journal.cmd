@echo off
set "lore_journal=%LOCALAPPDATA%\EldenRingLorePickup\index.html"
if exist "%lore_journal%" (
  start "" "%lore_journal%"
) else (
  echo The journal has not been created yet.
  echo Launch the game and collect an item that shows a lore card.
  echo If collection is paused, set a unique profile in LorePickup.ini and restart.
  pause
)
