from telethon.sync import TelegramClient

api_id = 2040
api_hash = "b18441a1ff607e10a989891a5462e627"

with TelegramClient("session", api_id, api_hash) as client:
    print("Logged in successfully")
    for dialog in client.iter_dialogs():
        print(dialog.name)
