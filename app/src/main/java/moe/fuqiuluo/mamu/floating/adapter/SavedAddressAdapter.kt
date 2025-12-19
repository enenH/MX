package moe.fuqiuluo.mamu.floating.adapter

import android.annotation.SuppressLint
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import moe.fuqiuluo.mamu.R
import moe.fuqiuluo.mamu.floating.data.model.SavedAddress
import moe.fuqiuluo.mamu.databinding.ItemSavedAddressBinding
import moe.fuqiuluo.mamu.floating.data.local.MemoryBackupManager
import moe.fuqiuluo.mamu.floating.data.model.DisplayValueType

class SavedAddressAdapter(
    private val onItemClick: (SavedAddress, Int) -> Unit = { _, _ -> },
    private val onFreezeToggle: (SavedAddress, Boolean) -> Unit = { _, _ -> },
    private val onItemDelete: (SavedAddress) -> Unit = { _ -> },
    private val onSelectionChanged: (Int) -> Unit = {}
) : RecyclerView.Adapter<SavedAddressAdapter.ViewHolder>() {

    private val addresses = mutableListOf<SavedAddress>()
    private val selectedPositions = mutableSetOf<Int>()

    fun setAddresses(newAddresses: List<SavedAddress>) {
        val oldSize = addresses.size
        addresses.clear()
        selectedPositions.clear()
        if (oldSize > 0) {
            notifyItemRangeRemoved(0, oldSize)
        }

        addresses.addAll(newAddresses)
        if (newAddresses.isNotEmpty()) {
            notifyItemRangeInserted(0, newAddresses.size)
        }
        onSelectionChanged(0)
    }

    fun addAddress(address: SavedAddress) {
        addresses.add(address)
        notifyItemInserted(addresses.size - 1)
    }

    fun updateAddress(address: SavedAddress) {
        val index = addresses.indexOfFirst { it.address == address.address }
        if (index >= 0) {
            addresses[index] = address
            notifyItemChanged(index)
        }
    }

    fun getSelectedItems(): List<SavedAddress> {
        return selectedPositions.map { addresses[it] }
    }

    fun selectAll() {
        selectedPositions.clear()
        selectedPositions.addAll(addresses.indices)
        notifyDataSetChanged()
        onSelectionChanged(selectedPositions.size)
    }

    fun deselectAll() {
        selectedPositions.clear()
        notifyDataSetChanged()
        onSelectionChanged(0)
    }

    fun invertSelection() {
        val newSelection = addresses.indices.toSet() - selectedPositions
        selectedPositions.clear()
        selectedPositions.addAll(newSelection)
        notifyDataSetChanged()
        onSelectionChanged(selectedPositions.size)
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
        val binding = ItemSavedAddressBinding.inflate(
            LayoutInflater.from(parent.context),
            parent,
            false
        )
        return ViewHolder(binding)
    }

    override fun onBindViewHolder(holder: ViewHolder, position: Int) {
        holder.bind(addresses[position])
    }

    override fun getItemCount(): Int = addresses.size

    inner class ViewHolder(
        private val binding: ItemSavedAddressBinding
    ) : RecyclerView.ViewHolder(binding.root) {

        @SuppressLint("SetTextI18n")
        fun bind(address: SavedAddress) {
            val context = binding.root.context
            val position = bindingAdapterPosition

            // 设置 checkbox 状态
            binding.checkbox.setOnCheckedChangeListener(null)
            val isSelected = selectedPositions.contains(position)
            binding.checkbox.isChecked = isSelected
            updateItemBackground(isSelected)
            binding.checkbox.setOnCheckedChangeListener { _, isChecked ->
                if (isChecked) {
                    selectedPositions.add(position)
                } else {
                    selectedPositions.remove(position)
                }
                updateItemBackground(isChecked)
                onSelectionChanged(selectedPositions.size)
            }

            // 设置变量名称
            binding.nameText.text = address.name

            // 设置地址（大写，无0x前缀）
            binding.addressText.text = String.format("%X", address.address)

            // 设置值
            binding.valueText.text = address.value.ifBlank { "空空如也" }

            // 备份值（旧值）
            val backup = MemoryBackupManager.getBackup(address.address)
            if (backup != null) {
                binding.backupValueText.text = "(${backup.originalValue})"
                binding.backupValueText.visibility = View.VISIBLE
            } else {
                binding.backupValueText.visibility = View.GONE
            }

            // 设置数据类型和范围
            val valueType = address.displayValueType ?: DisplayValueType.DWORD
            binding.typeText.text = valueType.code
            binding.typeText.setTextColor(valueType.textColor)
            binding.rangeText.text = address.range.code
            binding.rangeText.setTextColor(address.range.color)

            // 设置冻结按钮状态
            binding.freezeButton.apply {
                if (address.isFrozen) {
                    setIconResource(R.drawable.icon_play_arrow_24px)
                } else {
                    setIconResource(R.drawable.icon_pause_24px)
                }

                setOnClickListener {
                    val newFrozenState = !address.isFrozen
                    address.isFrozen = newFrozenState
                    // 立即更新UI
                    if (newFrozenState) {
                        setIconResource(R.drawable.icon_play_arrow_24px)
                    } else {
                        setIconResource(R.drawable.icon_pause_24px)
                    }
                    onFreezeToggle(address, newFrozenState)
                }
            }

            // 设置删除按钮
            binding.deleteButton.setOnClickListener {
                onItemDelete(address)
            }

            // 设置点击事件
            binding.itemContainer.setOnClickListener {
                onItemClick(address, position)
            }
        }

        private fun updateItemBackground(isSelected: Boolean) {
            binding.itemContainer.setBackgroundColor(
                if (isSelected) 0x33448AFF else 0x00000000
            )
        }
    }
}
