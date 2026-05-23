import random

def generate_phrase():
    subjects = ["Программист", "Кот", "Алгоритм", "Компилятор", "Сервер"]
    verbs = ["пишет", "изучает", "оптимизирует", "скрывает", "запускает"]
    objects = ["код", "структуры данных", "ошибки", "нейросети", "циклы"]
    
    phrase = f"{random.choice(subjects)} {random.choice(verbs)} {random.choice(objects)}."
    return phrase

def draw_tree(height):
    for i in range(height):
        spaces = " " * (height - i - 1)
        stars = "*" * (2 * i + 1)
        print(spaces + stars)
    
    trunk_spaces = " " * (height - 1)
    print(trunk_spaces + "|")

if __name__ == "__main__":
    print(generate_phrase())
    draw_tree(7)