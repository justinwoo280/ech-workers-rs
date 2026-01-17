cmake_minimum_required(VERSION 3.16)
project(ech-workers-gui VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Widgets)

set(SOURCES
    src/main.cpp
    src/mainwindow.cpp
    src/processmanager.cpp
    src/configmanager.cpp
    src/traymanager.cpp
    src/settingsdialog.cpp
    src/nodemanager.cpp
    src/systemproxy.cpp
    src/nodedialog.cpp
)

set(HEADERS
    include/mainwindow.h
    include/processmanager.h
    include/configmanager.h
    include/traymanager.h
    include/settingsdialog.h
    include/nodemanager.h
    include/systemproxy.h
    include/nodedialog.h
)

set(UI_FILES
    ui/mainwindow.ui
    ui/settingsdialog.ui
)

set(RESOURCES
    resources/resources.qrc
)

add_executable(${PROJECT_NAME}
    ${SOURCES}
    ${HEADERS}
    ${UI_FILES}
    ${RESOURCES}
)

target_include_directories(${PROJECT_NAME} PRIVATE include)

target_link_libraries(${PROJECT_NAME} PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Widgets
)

if(WIN32)
    set_target_properties(${PROJECT_NAME} PROPERTIES
        WIN32_EXECUTABLE TRUE
    )
endif()

install(TARGETS ${PROJECT_NAME}
    BUNDLE DESTINATION .
    RUNTIME DESTINATION bin
)
