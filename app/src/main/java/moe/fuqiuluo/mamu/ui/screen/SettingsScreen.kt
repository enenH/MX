package moe.fuqiuluo.mamu.ui.screen

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Brightness6
import androidx.compose.material.icons.filled.ColorLens
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.School
import androidx.compose.material.icons.filled.Window
import androidx.compose.material3.*
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.tencent.mmkv.MMKV
import moe.fuqiuluo.mamu.data.settings.autoStartFloatingWindow
import moe.fuqiuluo.mamu.ui.theme.AdaptiveLayoutInfo
import moe.fuqiuluo.mamu.ui.theme.AppTheme
import moe.fuqiuluo.mamu.ui.theme.DarkMode
import moe.fuqiuluo.mamu.ui.theme.Dimens
import moe.fuqiuluo.mamu.ui.theme.ThemeManager
import moe.fuqiuluo.mamu.ui.theme.rememberAdaptiveLayoutInfo
import moe.fuqiuluo.mamu.ui.tutorial.TutorialManager

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    windowSizeClass: WindowSizeClass,
    onNavigateBack: () -> Unit,
    onShowAbout: () -> Unit
) {
    val adaptiveLayout = rememberAdaptiveLayoutInfo(windowSizeClass)
    BackHandler(onBack = onNavigateBack)

    val useDynamicColor by ThemeManager.useDynamicColor.collectAsState()
    val currentTheme by ThemeManager.currentTheme.collectAsState()
    val darkMode by ThemeManager.darkMode.collectAsState()
    var showThemeDialog by remember { mutableStateOf(false) }
    var showDarkModeDialog by remember { mutableStateOf(false) }
    var showTutorialResetDialog by remember { mutableStateOf(false) }

    val mmkv = remember { MMKV.defaultMMKV() }
    var autoStartFloating by remember { mutableStateOf(mmkv.autoStartFloatingWindow) }

    if (showThemeDialog) {
        ThemeSelectionDialog(
            adaptiveLayout = adaptiveLayout,
            currentTheme = currentTheme,
            onThemeSelected = { theme ->
                ThemeManager.setTheme(theme)
                showThemeDialog = false
            },
            onDismiss = { showThemeDialog = false }
        )
    }

    if (showDarkModeDialog) {
        DarkModeSelectionDialog(
            adaptiveLayout = adaptiveLayout,
            currentMode = darkMode,
            onModeSelected = { mode ->
                ThemeManager.setDarkMode(mode)
                showDarkModeDialog = false
            },
            onDismiss = { showDarkModeDialog = false }
        )
    }

    if (showTutorialResetDialog) {
        AlertDialog(
            onDismissRequest = { showTutorialResetDialog = false },
            icon = {
                Icon(
                    imageVector = Icons.Default.School,
                    contentDescription = null
                )
            },
            title = { Text("重新开始教程") },
            text = { Text("教程已重置，返回首页后将再次显示新手教程。") },
            confirmButton = {
                TextButton(
                    onClick = {
                        TutorialManager.resetTutorial()
                        showTutorialResetDialog = false
                        onNavigateBack()
                    }
                ) {
                    Text("返回首页")
                }
            },
            dismissButton = {
                TextButton(onClick = { showTutorialResetDialog = false }) {
                    Text("取消")
                }
            }
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("设置") },
                navigationIcon = {
                    IconButton(onClick = onNavigateBack) {
                        Icon(
                            imageVector = Icons.Default.ArrowBack,
                            contentDescription = "返回"
                        )
                    }
                }
            )
        }
    ) { paddingValues ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Column(
                modifier = Modifier
                    .widthIn(
                        max = when (adaptiveLayout.windowSizeClass.widthSizeClass) {
                            WindowWidthSizeClass.Compact -> adaptiveLayout.contentMaxWidth
                            else -> 720.dp // 横屏时使用更宽的最大宽度
                        }
                    )
                    .fillMaxWidth()
                    .padding(paddingValues)
            ) {
                // 通用设置分组
                SettingsGroup(
                    adaptiveLayout = adaptiveLayout,
                    title = "通用"
                ) {
                    SettingsSwitchItem(
                        adaptiveLayout = adaptiveLayout,
                        icon = Icons.Default.Window,
                        title = "自动显示悬浮窗",
                        description = "应用启动时自动显示悬浮窗",
                        checked = autoStartFloating,
                        onCheckedChange = { enabled ->
                            autoStartFloating = enabled
                            mmkv.autoStartFloatingWindow = enabled
                        }
                    )

                    SettingsClickableItem(
                        adaptiveLayout = adaptiveLayout,
                        icon = Icons.Default.School,
                        title = "重新进入教程",
                        description = "重新显示新手教程",
                        onClick = { showTutorialResetDialog = true }
                    )

                    SettingsClickableItem(
                        adaptiveLayout = adaptiveLayout,
                        icon = Icons.Default.Info,
                        title = "关于 Mamu",
                        description = "查看版本信息、开源依赖和致谢",
                        onClick = {
                            onNavigateBack()
                            onShowAbout()
                        }
                    )
                }

                // 外观设置分组
                SettingsGroup(
                    adaptiveLayout = adaptiveLayout,
                    title = "外观"
                ) {
                    SettingsClickableItem(
                        adaptiveLayout = adaptiveLayout,
                        icon = Icons.Default.Brightness6,
                        title = "深色模式",
                        description = when (darkMode) {
                            DarkMode.FOLLOW_SYSTEM -> "跟随系统"
                            DarkMode.LIGHT -> "浅色"
                            DarkMode.DARK -> "深色"
                        },
                        onClick = { showDarkModeDialog = true }
                    )

                    SettingsClickableItem(
                        adaptiveLayout = adaptiveLayout,
                        icon = Icons.Default.ColorLens,
                        title = "主题",
                        description = stringResource(currentTheme.displayNameRes),
                        onClick = { showThemeDialog = true }
                    )

                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                        SettingsSwitchItem(
                            adaptiveLayout = adaptiveLayout,
                            icon = Icons.Default.Palette,
                            title = "动态配色",
                            description = "根据壁纸自动生成主题颜色",
                            checked = useDynamicColor,
                            onCheckedChange = { ThemeManager.setUseDynamicColor(it) }
                        )
                    } else {
                        SettingsInfoItem(
                            adaptiveLayout = adaptiveLayout,
                            icon = Icons.Default.Palette,
                            title = "动态配色",
                            description = "需要 Android 12 及以上版本"
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun SettingsGroup(
    adaptiveLayout: AdaptiveLayoutInfo,
    title: String,
    content: @Composable ColumnScope.() -> Unit
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(
                horizontal = Dimens.paddingLg(adaptiveLayout),
                vertical = Dimens.paddingSm(adaptiveLayout)
            ),
            fontWeight = FontWeight.Bold
        )
        content()
    }
}

@Composable
fun SettingsSwitchItem(
    adaptiveLayout: AdaptiveLayoutInfo,
    icon: ImageVector,
    title: String,
    description: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = Dimens.paddingLg(adaptiveLayout),
                vertical = Dimens.paddingMd(adaptiveLayout)
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(Dimens.iconMd(adaptiveLayout))
        )
        Spacer(modifier = Modifier.width(Dimens.spacingLg(adaptiveLayout)))
        Column(
            modifier = Modifier.weight(1f)
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Spacer(modifier = Modifier.width(Dimens.spacingLg(adaptiveLayout)))
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange
        )
    }
}

@Composable
fun SettingsInfoItem(
    adaptiveLayout: AdaptiveLayoutInfo,
    icon: ImageVector,
    title: String,
    description: String
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = Dimens.paddingLg(adaptiveLayout),
                vertical = Dimens.paddingMd(adaptiveLayout)
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(Dimens.iconMd(adaptiveLayout))
        )
        Spacer(modifier = Modifier.width(Dimens.spacingLg(adaptiveLayout)))
        Column(
            modifier = Modifier.weight(1f)
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

@Composable
fun SettingsClickableItem(
    adaptiveLayout: AdaptiveLayoutInfo,
    icon: ImageVector,
    title: String,
    description: String,
    onClick: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                horizontal = Dimens.paddingLg(adaptiveLayout),
                vertical = Dimens.paddingMd(adaptiveLayout)
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(Dimens.iconMd(adaptiveLayout))
        )
        Spacer(modifier = Modifier.width(Dimens.spacingLg(adaptiveLayout)))
        Column(
            modifier = Modifier.weight(1f)
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium
            )
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

// Dialog 使用响应式尺寸
@Composable
fun ThemeSelectionDialog(
    adaptiveLayout: AdaptiveLayoutInfo,
    currentTheme: AppTheme,
    onThemeSelected: (AppTheme) -> Unit,
    onDismiss: () -> Unit
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("选择主题") },
        text = {
            Column(
                modifier = Modifier.fillMaxWidth()
            ) {
                AppTheme.entries.forEach { theme ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onThemeSelected(theme) }
                            .padding(vertical = Dimens.spacingMd(adaptiveLayout)),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        RadioButton(
                            selected = theme == currentTheme,
                            onClick = { onThemeSelected(theme) }
                        )
                        Spacer(modifier = Modifier.width(Dimens.spacingSm(adaptiveLayout)))
                        Column {
                            Text(
                                text = stringResource(theme.displayNameRes),
                                style = MaterialTheme.typography.bodyLarge,
                                fontWeight = if (theme == currentTheme) FontWeight.Bold else FontWeight.Normal
                            )
                            Text(
                                text = stringResource(theme.descriptionRes),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text("取消")
            }
        }
    )
}

@Composable
fun DarkModeSelectionDialog(
    adaptiveLayout: AdaptiveLayoutInfo,
    currentMode: DarkMode,
    onModeSelected: (DarkMode) -> Unit,
    onDismiss: () -> Unit
) {
    val options = listOf(
        DarkMode.FOLLOW_SYSTEM to "跟随系统",
        DarkMode.LIGHT to "浅色",
        DarkMode.DARK to "深色"
    )

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("深色模式") },
        text = {
            Column(
                modifier = Modifier.fillMaxWidth()
            ) {
                options.forEach { (mode, label) ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onModeSelected(mode) }
                            .padding(vertical = Dimens.spacingMd(adaptiveLayout)),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        RadioButton(
                            selected = mode == currentMode,
                            onClick = { onModeSelected(mode) }
                        )
                        Spacer(modifier = Modifier.width(Dimens.spacingSm(adaptiveLayout)))
                        Text(
                            text = label,
                            style = MaterialTheme.typography.bodyLarge,
                            fontWeight = if (mode == currentMode) FontWeight.Bold else FontWeight.Normal
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text("取消")
            }
        }
    )
}
