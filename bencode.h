typedef struct {
    void *ptr;
    char type;
} Object;

typedef struct Node {
    void *data;
    struct Node *next;
} Node;

typedef struct {
    Node *head;
    Node *tail;
} List;

typedef struct {
    char *key;
    void *val;
} Pair;

void append(List *list, void *data);

int *parse_int(char *str);

int get_int_len(char *str);

char *parse_str(char *str);

int get_str_len(char *str);

// uses parser advancing logic
int get_list_len(char *str);

List *parse_list(char *str);

Pair *parse_pair(char *str);

int get_pair_len(char *str);

List *parse_dict(char *str);

int get_dict_len(char *str);

List *parse(char *str);

void print_dict(List *dict);

void print_list(List *list);

void print_tree(List *tree);

void free_ll(List *tree);