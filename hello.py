import os
import sys

# Список папок, которые по умолчанию стоит игнорировать, чтобы не спамить в консоль
DEFAULT_IGNORE = {'target', '.git'}

def print_tree(path, prefix='', ignore_dirs=DEFAULT_IGNORE):
    if not os.path.exists(path):
        print(f"Путь '{path}' не существует.")
        return

    # Получаем список всех элементов в директории и сортируем их (папки сначала)
    try:
        items = sorted(os.listdir(path), key=lambda x: (not os.path.isdir(os.path.join(path, x)), x.lower()))
    except PermissionError:
        # Если нет прав на чтение папки, просто пропускаем её
        return

    # Фильтруем игнорируемые папки
    items = [item for item in items if item not in ignore_dirs]
    
    total_items = len(items)
    
    for index, item in enumerate(items):
        item_path = os.path.join(path, item)
        is_last = (index == total_items - 1)
        
        # Выбираем правильный символ для ветки
        connector = '└── ' if is_last else '├── '
        print(f"{prefix}{connector}{item}")
        
        # Если это директория, уходим в рекурсию
        if os.path.isdir(item_path):
            # Для разветвлений подследников добавляем палочку, для последнего — пустоту
            extension = '    ' if is_last else '│   '
            print_tree(item_path, prefix + extension, ignore_dirs)

if __name__ == '__main__':
    # Если путь не передан аргументом, берем текущую папку
    target_dir = sys.argv[1] if len(sys.argv) > 1 else '.'
    
    print(os.path.abspath(target_dir))
    print_tree(target_dir)