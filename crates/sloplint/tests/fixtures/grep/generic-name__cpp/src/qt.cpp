struct Gadget {
    Q_GADGET
public:
    void process() {}
};

class Object : public QObject {
public:
    Q_OBJECT
public:
    void handle() {}
};

struct DeclaredGadget {
    Q_GADGET
public:
    void manage();
};
void DeclaredGadget::manage() {}

class DeclaredObject : public QObject {
public:
    Q_OBJECT
public:
    void perform();
};
void DeclaredObject::perform() {}
