lappend auto_path "C:/lscc/radiant/2024.1/scripts/tcl/simulation"
package require simulation_generation
set ::bali::simulation::Para(DEVICEPM) {je5d00}
set ::bali::simulation::Para(DEVICEFAMILYNAME) {LIFCL}
set ::bali::simulation::Para(PROJECT) {radiosim}
set ::bali::simulation::Para(MDOFILE) {}
set ::bali::simulation::Para(PROJECTPATH) {C:/git/radio/radiohw/radiosim}
set ::bali::simulation::Para(FILELIST) {"C:/git/radio/radiohw/pcie_1x/rtl/pcie_1x.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/csi_dphy.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/dphy_dummy.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/int_osc.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/line_ram_dp.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/out_line_ram_dp.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/rom_first.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/Lattice_specfic/rom_sec.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/camera_controller.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/debayer_filter.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/frame_detector.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/lsync_reset_generator.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_16_nx.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_byte_aligner.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_lane_aligner.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_packet_decoder_8b2lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_packet_decoder_8b4lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_packet_decoder_16b2lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_packet_decoder_16b4lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_raw_depacker_8b2lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_raw_depacker_8b2lane_2ppc.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_raw_depacker_8b4lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_raw_depacker_8b4lane_8ppc.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_raw_depacker_16b2lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/mipi_csi_rx_raw_depacker_16b4lane.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/output_reformatter.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/rgb_to_yuv.v" "C:/git/radio/radiohw/USB_C_Industrial_Camera_FPGA_USB3/FPGA_Firmware/Source/src/sample_generator.v" "C:/git/radio/radiohw/source/impl_1/por.vhd" "C:/git/radio/radiohw/source/impl_1/main.vhd" "C:/git/radio/radiohw/source/impl_1/main_tb.vhd" }
set ::bali::simulation::Para(GLBINCLIST) {}
set ::bali::simulation::Para(INCLIST) {"none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none" "none"}
set ::bali::simulation::Para(WORKLIBLIST) {"work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" "work" }
set ::bali::simulation::Para(COMPLIST) {"VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VERILOG" "VHDL" "VHDL" "VHDL" }
set ::bali::simulation::Para(LANGSTDLIST) {"Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "Verilog 2001" "VHDL_2008" "VHDL_2008" "VHDL_2008" }
set ::bali::simulation::Para(SIMLIBLIST) {pmi_work ovi_lifcl}
set ::bali::simulation::Para(MACROLIST) {}
set ::bali::simulation::Para(SIMULATIONTOPMODULE) {radio_tb}
set ::bali::simulation::Para(SIMULATIONINSTANCE) {}
set ::bali::simulation::Para(LANGUAGE) {VHDL}
set ::bali::simulation::Para(SDFPATH)  {}
set ::bali::simulation::Para(INSTALLATIONPATH) {C:/lscc/radiant/2024.1}
set ::bali::simulation::Para(MEMPATH) {C:/git/radio/radiohw/pcie_1x}
set ::bali::simulation::Para(UDOLIST) {}
set ::bali::simulation::Para(ADDTOPLEVELSIGNALSTOWAVEFORM)  {1}
set ::bali::simulation::Para(RUNSIMULATION)  {1}
set ::bali::simulation::Para(SIMULATIONTIME)  {100}
set ::bali::simulation::Para(SIMULATIONTIMEUNIT)  {ns}
set ::bali::simulation::Para(SIMULATION_RESOLUTION)  {default}
set ::bali::simulation::Para(NOGUI) {0}
set ::bali::simulation::Para(ISRTL)  {1}
set ::bali::simulation::Para(HDLPARAMETERS) {}
set ::bali::simulation::Para(AUTOORDER)  {1}
set ::bali::simulation::Para(PERMISSIVE)  {0}
set ::bali::simulation::Para(OPTIMIZATION_DEBUG)  {1}
::bali::simulation::QuestaSim_Q_Run
