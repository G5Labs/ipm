"""Teaching fixture, not production code: one operation, no concurrent writers."""


class Store:
    def __init__(self):
        self.phase = "idle"
        self.stored = 0

    def stage(self):
        if self.phase != "idle":
            raise ValueError("only an idle store can stage")
        self.phase = "staged"

    def publish(self, *, bad_write=False):
        if self.phase != "staged":
            raise ValueError("only a staged store can publish")
        self.stored = 2 if bad_write else 1
        self.phase = "done"

    def commit_directly(self):
        if self.phase != "idle":
            raise ValueError("only an idle store can commit directly")
        self.stored = 1
        self.phase = "done"

    def read(self):
        return self.phase == "done", self.stored
