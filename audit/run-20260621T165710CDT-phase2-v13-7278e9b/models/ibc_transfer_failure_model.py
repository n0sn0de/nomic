#!/usr/bin/env python3
"""Executable model for Nomic IbcDest.transfer() failure handling.

The source mints coins into the local IBC transfer account before calling
deliver_message(), and converts any deliver_message() error into a debug log and
Ok(()). This model checks the state left when the send path fails before escrow.
"""


class TransferModule:
    def __init__(self):
        self.accounts = {}
        self.packet_sent = False

    def mint_coins_execute(self, sender, amount):
        self.accounts[sender] = self.accounts.get(sender, 0) + amount

    def deliver_message(self, should_fail):
        if should_fail:
            raise ValueError("channel not found")
        self.packet_sent = True
        self.accounts["sender"] -= 100


def ibc_dest_transfer_v13(module, sender, amount, should_fail):
    module.mint_coins_execute(sender, amount)
    try:
        module.deliver_message(should_fail)
    except ValueError as err:
        return f"logged and swallowed: {err}"
    return "sent"


def main():
    module = TransferModule()
    outcome = ibc_dest_transfer_v13(module, "sender", 100, should_fail=True)
    print("outcome:", outcome)
    print("sender_ibc_balance:", module.accounts.get("sender", 0))
    print("packet_sent:", module.packet_sent)

    assert outcome.startswith("logged and swallowed")
    assert module.accounts["sender"] == 100
    assert module.packet_sent is False


if __name__ == "__main__":
    main()
